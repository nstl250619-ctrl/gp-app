#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""GP 隧道常驻监控守护进程。

解决的问题：tunnel_monitor.py 是「一次性检测器」，跑完 --duration 就退出。
本脚本让它变成常驻：分轮采集（默认每小时一轮，每轮独立 CSV 方便归档），
子进程异常退出后自动重启，永不主动结束。

用法：
    # 前台（Ctrl+C 停止）
    python tools/tunnel_monitor_daemon.py --key sk-xxx

    # 后台常驻（推荐，无窗口）
    pythonw tools/tunnel_monitor_daemon.py --key sk-xxx --round-s 3600

    # 不带 key：只监控 GP 进程 / ssh 进程 / 端口 / 断线重连（不测端到端延迟）
    pythonw tools/tunnel_monitor_daemon.py

key 来源优先级：--key > 环境变量 GP_API_KEY > tools/.gp_key 文件

输出：
    logs/gp_tunnel_<时间>.csv / _report.json   每轮原始数据与报告
    logs/monitor_daemon.log                    守护进程自身日志（追加）
    logs/monitor_daemon.pid                    当前 PID（用于停止）
"""

import argparse
import csv
import datetime as dt
import glob
import json
import os
import signal
import subprocess
import sys
import time

if sys.stdout and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:  # noqa: BLE001
        pass

HERE = os.path.dirname(os.path.abspath(__file__))
MONITOR = os.path.join(HERE, "tunnel_monitor.py")
DEFAULT_OUT = os.path.join(os.path.dirname(HERE), "logs")
NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0) if os.name == "nt" else 0

_child = None
_stop = False


def log(msg, fp=None):
    line = f"[{dt.datetime.now():%Y-%m-%d %H:%M:%S}] {msg}"
    print(line, flush=True)
    if fp:
        try:
            fp.write(line + "\n")
            fp.flush()
        except Exception:  # noqa: BLE001
            pass


def resolve_key(cli_key):
    if cli_key:
        return cli_key.strip()
    env = os.environ.get("GP_API_KEY", "").strip()
    if env:
        return env
    path = os.path.join(HERE, ".gp_key")
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            return f.read().strip()
    return ""


def cleanup_old(out_dir, keep_days):
    if keep_days <= 0:
        return
    cutoff = time.time() - keep_days * 86400
    removed = 0
    for pat in ("gp_tunnel_*.csv", "gp_tunnel_*_report.json"):
        for p in glob.glob(os.path.join(out_dir, pat)):
            try:
                if os.path.getmtime(p) < cutoff:
                    os.remove(p)
                    removed += 1
            except Exception:  # noqa: BLE001
                pass
    if removed:
        log(f"清理 {removed} 个超过 {keep_days} 天的旧结果文件")


def summarize(out_dir, since, lf):
    """把本次会话内各轮 report 合并成一份总报告。"""
    rounds = []
    for p in sorted(glob.glob(os.path.join(out_dir, "gp_tunnel_*.csv"))):
        try:
            if os.path.getmtime(p) < since - 10:
                continue
            with open(p, encoding="utf-8-sig", errors="replace") as f:
                rows = list(csv.DictReader(f))
        except Exception:  # noqa: BLE001
            continue
        if not rows:
            continue

        def truth(v):
            return str(v).strip().lower() in ("true", "1", "1.0")

        n = len(rows)
        listen = sum(1 for r in rows if truth(r.get("port_listening")))
        gp_on = sum(1 for r in rows if truth(r.get("gp_running")))
        down = sum(1 for i in range(1, n)
                   if truth(rows[i - 1].get("port_listening")) and not truth(rows[i].get("port_listening")))
        up = sum(1 for i in range(1, n)
                 if not truth(rows[i - 1].get("port_listening")) and truth(rows[i].get("port_listening")))
        gp_exit = sum(1 for i in range(1, n)
                      if truth(rows[i - 1].get("gp_running")) and not truth(rows[i].get("gp_running")))

        reconn = 0
        prev = None
        for r in rows:
            cur = (r.get("ssh_pids") or "").strip()
            if prev and cur and cur != prev:
                reconn += 1
            if cur:
                prev = cur

        lats = []
        for r in rows:
            v = (r.get("latency_ms") or "").strip()
            if v:
                try:
                    lats.append(float(v))
                except ValueError:
                    pass
        has_key = bool(lats)
        http_ok = sum(1 for r in rows if truth(r.get("http_ok"))) if has_key else 0
        http_fail = (n - http_ok) if has_key else 0

        try:
            t0 = dt.datetime.strptime(rows[0]["time"], "%Y-%m-%d %H:%M:%S")
            t1 = dt.datetime.strptime(rows[-1]["time"], "%Y-%m-%d %H:%M:%S")
            window = (t1 - t0).total_seconds()
        except Exception:  # noqa: BLE001
            t0 = t1 = None
            window = 0.0

        rounds.append({
            "file": os.path.basename(p), "samples": n,
            "listening_pct": round(listen / n * 100, 1), "gp_pct": round(gp_on / n * 100, 1),
            "down": down, "up": up, "gp_exit": gp_exit, "reconn": reconn,
            "has_key": has_key, "http_ok": http_ok, "http_fail": http_fail,
            "lat_sum": sum(lats), "lat_n": len(lats), "window_s": window,
            "started": t0.strftime("%Y-%m-%d %H:%M:%S") if t0 else None,
            "ended": t1.strftime("%Y-%m-%d %H:%M:%S") if t1 else None,
        })

    if not rounds:
        log("汇总：本次会话没有可用 CSV", lf)
        return

    total = sum(r["samples"] for r in rounds) or 1
    lat_n = sum(r["lat_n"] for r in rounds)
    lat_sum = sum(r["lat_sum"] for r in rounds)
    key_rounds = [r for r in rounds if r["has_key"]]
    summary = {
        "rounds": len(rounds),
        "samples": total,
        "window_s": round(sum(r["window_s"] for r in rounds), 1),
        "started": rounds[0]["started"],
        "ended": rounds[-1]["ended"],
        "has_key": bool(key_rounds),
        "tunnel": {
            "listening_pct": round(sum(r["listening_pct"] * r["samples"] for r in rounds) / total, 1),
            "down_events": sum(r["down"] for r in rounds),
            "up_events": sum(r["up"] for r in rounds),
            "ssh_reconnects": sum(r["reconn"] for r in rounds),
        },
        "gp": {"exit_events": sum(r["gp_exit"] for r in rounds)},
        "http": {
            "ok": sum(r["http_ok"] for r in key_rounds),
            "fail": sum(r["http_fail"] for r in key_rounds),
            "rounds_with_key": len(key_rounds),
        },
        "latency_ms": {"samples": lat_n, "avg": round(lat_sum / lat_n, 1) if lat_n else None},
        "rounds_detail": [{k: r[k] for k in ("file", "samples", "listening_pct",
                                             "down", "up", "gp_exit", "reconn")} for r in rounds],
    }
    jp = os.path.join(out_dir, "tunnel_session_summary.json")
    with open(jp, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)

    t = summary["tunnel"]
    md = [
        "# GP 隧道会话汇总",
        "",
        f"- 轮次 {summary['rounds']} | 采样 {summary['samples']} 个 | 累计窗口 {summary['window_s']}s",
        f"- 区间 {summary['started']} → {summary['ended']}",
        f"- 隧道监听 **{t['listening_pct']}%** | 断线 {t['down_events']} 次 | 恢复 {t['up_events']} 次 | ssh 重连 {t['ssh_reconnects']} 次",
        f"- GP 进程退出 {summary['gp']['exit_events']} 次",
        f"- HTTP 成功/失败 {summary['http']['ok']} / {summary['http']['fail']}"
        f"（仅统计有 key 的 {summary['http']['rounds_with_key']} 轮）",
        f"- 延迟均值 {summary['latency_ms']['avg']} ms（{summary['latency_ms']['samples']} 个样本）",
        f"- 是否含端到端延迟 {'是' if summary['has_key'] else '否（未配置 key）'}",
    ]
    with open(os.path.join(out_dir, "tunnel_session_summary.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(md) + "\n")

    log(f"会话汇总已写入 {jp}", lf)
    for line in md[2:]:
        log("  " + line.lstrip("- "), lf)


def on_signal(signum, _frame):
    global _stop
    _stop = True
    log(f"收到信号 {signum}，准备停止…")
    if _child and _child.poll() is None:
        try:
            _child.terminate()
            _child.wait(timeout=8)
        except Exception:  # noqa: BLE001
            try:
                _child.kill()
            except Exception:  # noqa: BLE001
                pass


def main():
    global _child
    ap = argparse.ArgumentParser(description="GP 隧道常驻监控（分轮采集 + 自动重启）")
    ap.add_argument("--key", default="", help="API key（不传则读 GP_API_KEY / tools/.gp_key）")
    ap.add_argument("--port", type=int, default=3000, help="隧道本地端口（默认 3000）")
    ap.add_argument("--interval", type=float, default=2.0, help="采样间隔秒（默认 2）")
    ap.add_argument("--round-s", type=float, default=3600, help="每轮采集时长秒（默认 3600，即每小时一个文件）")
    ap.add_argument("--restart-delay", type=float, default=10, help="轮次间歇秒（默认 10）")
    ap.add_argument("--out-dir", default=DEFAULT_OUT, help="输出目录（默认 greenpool/logs）")
    ap.add_argument("--keep-days", type=int, default=14, help="保留天数，0=不清理（默认 14）")
    ap.add_argument("--total-hours", type=float, default=0,
                    help="本次会话总时长小时数，到点自动收尾并汇总；0=一直跑（默认 0）")
    args = ap.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)
    started = time.time()
    deadline = started + args.total_hours * 3600 if args.total_hours > 0 else None

    log_path = os.path.join(args.out_dir, "monitor_daemon.log")
    pid_path = os.path.join(args.out_dir, "monitor_daemon.pid")
    with open(pid_path, "w", encoding="utf-8") as f:
        f.write(str(os.getpid()))

    signal.signal(signal.SIGINT, on_signal)
    if hasattr(signal, "SIGTERM"):
        signal.signal(signal.SIGTERM, on_signal)

    with open(log_path, "a", encoding="utf-8") as lf:
        lf.write("\n" + "=" * 70 + "\n")
        key0 = resolve_key(args.key)
        log(f"守护进程启动 pid={os.getpid()} | 每轮 {int(args.round_s)}s | "
            f"间隔 {args.interval}s | 端口 {args.port} | "
            f"总时长 {args.total_hours}h | "
            f"{'含端到端延迟' if key0 else '仅存活（无 key）'}", lf)
        log(f"监控脚本 {MONITOR}", lf)

        rd = 0
        while not _stop:
            if deadline is not None:
                remain = deadline - time.time()
                if remain <= 30:
                    log("已达到会话总时长，停止采集", lf)
                    break
                round_s = min(args.round_s, remain)
            else:
                round_s = args.round_s

            rd += 1
            cleanup_old(args.out_dir, args.keep_days)
            key = resolve_key(args.key)  # 每轮重读：中途写入 tools/.gp_key 可自动生效
            cmd = [sys.executable, MONITOR,
                   "--auto", "--port", str(args.port),
                   "--interval", str(args.interval),
                   "--duration", str(int(round_s)),
                   "--out-dir", args.out_dir]
            if key:
                cmd += ["--key", key]

            log(f"--- 第 {rd} 轮开始 ---", lf)
            try:
                _child = subprocess.Popen(
                    cmd, cwd=os.path.dirname(HERE),
                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                    text=True, encoding="utf-8", errors="replace",
                    creationflags=NO_WINDOW,
                )
                for line in _child.stdout:
                    lf.write(line if line.endswith("\n") else line + "\n")
                    lf.flush()
                rc = _child.wait()
                log(f"--- 第 {rd} 轮结束 rc={rc} ---", lf)
            except Exception as e:  # noqa: BLE001
                log(f"第 {rd} 轮异常: {e}", lf)

            if _stop:
                break
            time.sleep(max(1, args.restart_delay))

        try:
            summarize(args.out_dir, started, lf)
        except Exception as e:  # noqa: BLE001
            log(f"汇总失败: {e}", lf)
        log("守护进程已退出", lf)
    try:
        os.remove(pid_path)
    except Exception:  # noqa: BLE001
        pass


if __name__ == "__main__":
    main()
