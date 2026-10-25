#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""GP 直链加速隧道检测器（专用）。

零配置自动发现 GP 工具、隧道 ssh 进程、本地转发端口（默认 3000），
检测：隧道稳定性 / 重连次数 / 断线恢复 / 请求延迟 / 抖动 / GP 进程存活。

典型用法：
    # ① 先启动脚本（自动等隧道上线），再到 GP 里点「启动」直链加速
    python tools/tunnel_monitor.py --key sk-xxxx --auto

    # ② 隧道已在运行，直接测 5 分钟
    python tools/tunnel_monitor.py --key sk-xxxx --duration 300

    # ③ 不传 key：只检测隧道存活（GP 进程 / ssh 进程 / 端口监听 / 断线重连）
    python tools/tunnel_monitor.py --auto

    # ④ 测完按 Ctrl+C 提前结束，仍会输出完整报告

采集（每次采样一行）：
    GP 进程是否运行 | ssh 进程数/PID | 3000 端口监听 + owner PID |
    请求状态码 / 端到端延迟 ms | 隧道事件（上线/下线/重连）
输出：
    logs/gp_tunnel_<时间>.csv           原始样本
    logs/gp_tunnel_<时间>_report.json   结构化报告
    控制台：实时 + 结束时综合分析（含可用率、断线恢复、抖动、评级）
"""

import argparse
import csv
import datetime as dt
import http.client
import json
import os
import platform
import socket
import statistics
import subprocess
import sys
import time

if sys.stdout and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:  # noqa: BLE001
        pass

IS_WIN = platform.system() == "Windows"
NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0) if IS_WIN else 0
GP_PROC = "greenpool.exe"


# ---------------------------------------------------------------- 系统探测

def _tasklist(name):
    """返回指定进程名的 PID 列表（Windows tasklist / 其他 pgrep）。"""
    try:
        if IS_WIN:
            out = subprocess.run(
                ["tasklist", "/FI", f"IMAGENAME eq {name}", "/FO", "CSV", "/NH"],
                capture_output=True, text=True, timeout=10, creationflags=NO_WINDOW,
            ).stdout
            pids = []
            for line in out.splitlines():
                parts = [p.strip().strip('"') for p in line.split(",")]
                if parts and name.lower() in parts[0].lower():
                    try:
                        pids.append(int(parts[1]))
                    except (IndexError, ValueError):
                        pass
            return pids
        out = subprocess.run(["pgrep", "-f", name], capture_output=True, text=True, timeout=10).stdout
        return [int(x) for x in out.split() if x.isdigit()]
    except Exception:  # noqa: BLE001
        return []


def gp_pids():
    return _tasklist(GP_PROC)


def ssh_pids():
    return _tasklist("ssh.exe" if IS_WIN else "ssh")


def port_owner(port):
    """返回监听指定本地端口的 PID（无则 None）。Windows 用 netstat -ano。"""
    try:
        if IS_WIN:
            out = subprocess.run(
                ["netstat", "-ano", "-p", "tcp"],
                capture_output=True, text=True, timeout=10, creationflags=NO_WINDOW,
            ).stdout
        else:
            out = subprocess.run(["ss", "-tlnp"], capture_output=True, text=True, timeout=10).stdout
        for line in out.splitlines():
            if f":{port} " not in line and f":{port}\t" not in line:
                continue
            if "LISTEN" not in line.upper():
                continue
            parts = line.split()
            # Windows: TCP 0.0.0.0:3000 0.0.0.0:0 LISTENING 12345
            for p in reversed(parts):
                if p.isdigit():
                    return int(p)
            # Linux ss 情况（含 pid）
            import re
            m = re.search(r"pid=(\d+)", line)
            return int(m.group(1)) if m else None
    except Exception:  # noqa: BLE001
        pass
    return None


def port_listening(port, timeout=0.3):
    s = socket.socket()
    s.settimeout(timeout)
    try:
        s.connect(("127.0.0.1", port))
        return True
    except OSError:
        return False
    finally:
        s.close()


def probe(port, key, path, timeout):
    """经隧道发一次 HTTP 探测（有 key 才是真正的端到端）。"""
    t0 = time.perf_counter()
    try:
        conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
        headers = {"Content-Type": "application/json", "Accept": "*/*"}
        if key:
            headers["Authorization"] = f"Bearer {key}"
        conn.request("GET", path, headers=headers)
        resp = conn.getresponse()
        resp.read()
        lat = (time.perf_counter() - t0) * 1000.0
        ok = 200 <= resp.status < 300
        return ok, resp.status, lat, "" if ok else f"HTTP {resp.status}"
    except Exception as e:  # noqa: BLE001
        return False, None, (time.perf_counter() - t0) * 1000.0, str(e)[:100]


# ---------------------------------------------------------------- 分析

def _pct(values, p):
    if not values:
        return None
    vs = sorted(values)
    k = max(0, min(len(vs) - 1, int(round((p / 100.0) * (len(vs) - 1)))))
    return vs[k]


def analyze(samples, events, interval):
    total = len(samples)
    if not total:
        return None

    gp_on = [s for s in samples if s["gp_running"]]
    tunnel_up = [s for s in samples if s["port_listening"]]
    oks = [s for s in samples if s["http_ok"]]
    # 只取真正的数值延迟（无 key 时 latency_ms 为空字符串，不能参与算术）
    lats = [float(s["latency_ms"]) for s in oks if isinstance(s["latency_ms"], (int, float))]
    jitters = [abs(lats[i] - lats[i - 1]) for i in range(1, len(lats))]

    # 隧道在线期间（GP 运行 + 端口监听）的可用率
    tunnel_samples = [s for s in samples if s["port_listening"]]
    up_rate = (len(tunnel_samples) / total * 100.0) if total else 0.0

    # 断线事件与恢复时长
    downs = [e for e in events if e["type"] == "tunnel_down"]
    ups = [e for e in events if e["type"] == "tunnel_up"]
    recoveries = []
    for d in downs:
        after = [u for u in ups if u["ts"] > d["ts"]]
        if after:
            recoveries.append(after[0]["ts"] - d["ts"])
    reconnects = len([e for e in events if e["type"] == "ssh_reconnect"])

    if not key_lat_available(lats):
        grade = "存活可用（未测请求延迟）" if up_rate >= 95 else "存活不稳定"
    else:
        avg = statistics.mean(lats)
        grade = "A（优秀）" if up_rate >= 99 and avg < 2000 else ("B（可用）" if up_rate >= 95 else "C（不稳定）")

    return {
        "samples": total,
        "window_s": round(total * interval, 1),
        "gp": {
            "running_samples": len(gp_on),
            "running_pct": round(len(gp_on) / total * 100.0, 1),
            "exit_events": len([e for e in events if e["type"] == "gp_exit"]),
        },
        "tunnel": {
            "listening_pct": round(up_rate, 1),
            "down_events": len(downs),
            "up_events": len(ups),
            "ssh_reconnects": reconnects,
            "avg_recovery_s": round(statistics.mean(recoveries), 1) if recoveries else None,
            "max_recovery_s": round(max(recoveries), 1) if recoveries else None,
        },
        "latency_ms": {
            "avg": round(statistics.mean(lats), 1) if lats else None,
            "p50": round(_pct(lats, 50), 1) if lats else None,
            "p90": round(_pct(lats, 90), 1) if lats else None,
            "p99": round(_pct(lats, 99), 1) if lats else None,
            "max": round(max(lats), 1) if lats else None,
            "stdev": round(statistics.pstdev(lats), 1) if len(lats) > 1 else None,
        },
        "jitter_ms": {
            "avg_abs_diff": round(statistics.mean(jitters), 1) if jitters else None,
            "max_jump": round(max(jitters), 1) if jitters else None,
        },
        "http": {"ok": len(oks), "fail": total - len(oks)},
        "errors": sorted({s["error"] for s in samples if s["error"]})[:8],
        "grade": grade,
    }


def key_lat_available(lats):
    return len(lats) > 0


def print_report(rep, events):
    print("\n" + "=" * 70)
    print("GP 直链加速 · 隧道检测报告")
    print("=" * 70)
    gp, tu, lat, jit = rep["gp"], rep["tunnel"], rep["latency_ms"], rep["jitter_ms"]
    print(f"  采样           {rep['samples']} 个 / {rep['window_s']}s")
    print(f"  GP 进程        运行 {gp['running_pct']}% 的采样" +
          (f"（中途退出 {gp['exit_events']} 次）" if gp["exit_events"] else ""))
    print(f"  隧道监听        {tu['listening_pct']}% 的采样")
    print(f"  上线/下线       {tu['up_events']} 次 / {tu['down_events']} 次")
    if tu["down_events"]:
        print(f"  断线恢复        平均 {tu['avg_recovery_s']}s，最长 {tu['max_recovery_s']}s")
    print(f"  ssh 重连        {tu['ssh_reconnects']} 次")
    if lat["avg"] is not None:
        print(f"  请求延迟(ms)    平均 {lat['avg']} | P50 {lat['p50']} | P90 {lat['p90']} | P99 {lat['p99']} | 最大 {lat['max']}")
        print(f"                  标准差 {lat['stdev']}")
        print(f"  抖动(ms)        相邻均差 {jit['avg_abs_diff']} | 最大跳变 {jit['max_jump']}")
    else:
        print("  请求延迟        未测（没传 --key；仅验证了隧道存活）")
    print(f"  HTTP 成功/失败  {rep['http']['ok']} / {rep['http']['fail']}")
    if rep["errors"]:
        print("  主要错误")
        for e in rep["errors"]:
            print(f"    - {e}")
    print(f"  评级            {rep['grade']}")
    if events:
        print("  事件时间线（最近 10 条）")
        for e in events[-10:]:
            print(f"    [{e['time']}] {e['type']}  {e.get('detail', '')}")
    print("=" * 70)


# ---------------------------------------------------------------- 主流程

def main():
    ap = argparse.ArgumentParser(description="GP 直链加速隧道检测器（自动发现 GP/ssh/端口）")
    ap.add_argument("--key", default=os.environ.get("GP_API_KEY", ""), help="API key（不传则只测隧道存活，不测请求延迟）")
    ap.add_argument("--port", type=int, default=3000, help="GP 隧道本地转发端口（默认 3000）")
    ap.add_argument("--path", default="/v1/models", help="探测路径（默认 /v1/models，轻量）")
    ap.add_argument("--interval", type=float, default=2.0, help="采样间隔秒（默认 2）")
    ap.add_argument("--duration", type=float, default=0, help="总时长秒（0=无限，Ctrl+C 结束；默认 0）")
    ap.add_argument("--auto", action="store_true", help="自动模式：先等隧道上线（GP 里点启动）再开始采样")
    ap.add_argument("--timeout", type=float, default=15.0, help="单次请求超时秒（默认 15）")
    ap.add_argument("--out-dir", default="logs", help="输出目录（默认 logs）")
    args = ap.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)
    stamp = dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    csv_path = os.path.join(args.out_dir, f"gp_tunnel_{stamp}.csv")
    rep_path = os.path.join(args.out_dir, f"gp_tunnel_{stamp}_report.json")

    print(f"[GP 隧道检测] 端口 {args.port} | 间隔 {args.interval}s | "
          f"时长 {'无限（Ctrl+C 结束）' if args.duration <= 0 else str(int(args.duration)) + 's'} | "
          f"{'含请求延迟' if args.key else '仅隧道存活（未传 key）'}")
    print(f"[输出] {csv_path}\n")

    samples, events = [], []
    last_ssh = None
    last_port = None
    last_gp = None
    deadline = time.time() + args.duration if args.duration > 0 else None
    waiting = args.auto  # auto 模式：先等隧道上线

    def ev(kind, detail=""):
        e = {"ts": time.time(), "time": dt.datetime.now().strftime("%H:%M:%S"), "type": kind, "detail": detail}
        events.append(e)
        print(f"  >>> [{e['time']}] {kind} {detail}")

    with open(csv_path, "w", newline="", encoding="utf-8-sig") as f:
        w = csv.writer(f)
        w.writerow(["time", "gp_running", "gp_pid", "ssh_count", "ssh_pids", "port_listening",
                    "port_owner", "http_ok", "http_status", "latency_ms", "error"])

        try:
            while True:
                now = time.time()
                if deadline is not None and now >= deadline:
                    break

                gps = gp_pids()
                shs = ssh_pids()
                owner = port_owner(args.port)
                listening = owner is not None or port_listening(args.port)

                # auto 模式：等隧道上线（GP 运行 + 端口监听）后自动开始
                if waiting:
                    if gps and listening:
                        waiting = False
                        ev("start", "检测到隧道已上线，开始采集")
                    else:
                        time.sleep(max(0.5, args.interval))
                        continue

                if last_gp is not None and bool(gps) != bool(last_gp):
                    ev("gp_start" if gps else "gp_exit")
                last_gp = gps

                if last_ssh is not None and set(shs) != set(last_ssh):
                    if not last_ssh and shs:
                        ev("tunnel_up", f"ssh={shs}")
                    elif last_ssh and not shs:
                        ev("tunnel_down")
                    elif last_ssh and shs:
                        ev("ssh_reconnect", f"{sorted(last_ssh)} -> {sorted(shs)}")
                last_ssh = shs

                if last_port is not None and listening != last_port:
                    if listening:
                        ev("tunnel_up", f"port {args.port} listening")
                    else:
                        ev("tunnel_down", f"port {args.port} closed")
                last_port = listening

                if last_port is not None and listening and shs:
                    # 端口在 + ssh 在 → 有 key 就测端到端
                    if args.key:
                        ok, status, lat, err = probe(args.port, args.key, args.path, args.timeout)
                    else:
                        ok, status, lat, err = True, None, None, ""
                else:
                    ok, status, lat, err = False, None, None, "tunnel not up"

                row = {
                    "time": dt.datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
                    "gp_running": bool(gps),
                    "gp_pid": gps[0] if gps else "",
                    "ssh_count": len(shs),
                    "ssh_pids": ";".join(str(x) for x in shs),
                    "port_listening": listening,
                    "port_owner": owner or "",
                    "http_ok": ok,
                    "http_status": status or "",
                    "latency_ms": round(lat, 1) if lat is not None else "",
                    "error": err,
                }
                samples.append(row)
                w.writerow([row["time"], row["gp_running"], row["gp_pid"], row["ssh_count"],
                            row["ssh_pids"], row["port_listening"], row["port_owner"],
                            row["http_ok"], row["http_status"], row["latency_ms"], row["error"]])
                f.flush()

                if args.key:
                    lat_s = f"{row['latency_ms']:>7.1f}ms" if row["latency_ms"] != "" else "      -  "
                    print(f"[{row['time']}] {'OK ' if ok else 'FAIL'} {lat_s} gp={row['gp_pid']} ssh={row['ssh_count']} port={'UP' if listening else 'DOWN'} {err}")
                else:
                    print(f"[{row['time']}] gp={'ON ' if gps else 'OFF'} ssh={row['ssh_count']} port={'UP' if listening else 'DOWN'} owner={row['port_owner']}")

                time.sleep(max(0.05, args.interval))
        except KeyboardInterrupt:
            print("\n[stop] 已中断，生成报告…")

    if not samples:
        print("[warn] 没有采集到样本（隧道始终未上线？）")
        return

    rep = analyze(samples, events, args.interval)
    rep["meta"] = {
        "port": args.port,
        "has_key": bool(args.key),
        "interval_s": args.interval,
        "started": samples[0]["time"],
        "ended": samples[-1]["time"],
        "platform": platform.platform(),
        "gp_process": GP_PROC,
    }
    with open(rep_path, "w", encoding="utf-8") as f:
        json.dump(rep, f, ensure_ascii=False, indent=2)

    print_report(rep, events)
    print(f"\n原始样本  {csv_path}")
    print(f"汇总报告  {rep_path}")


if __name__ == "__main__":
    main()
