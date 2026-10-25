"""清理 CodeBuddy 本地状态里的自定义模型引用（custom-local:*）。

只动 4 个 map + 选中模型，改前自动备份 state.vscdb。
定义本身在云端账号，本地删除后仍需在 IDE 设置里移除定义。
"""
import json
import os
import shutil
import sqlite3
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

PREFIX = "custom-local:"
FIELDS = ("chatModelThinkingMap", "chatModelThinkingGlobalMap",
          "chatModelEffortMap", "chatModelContextLengthMap")
FALLBACK = "deepseek-v4.1-flash"
STAMP = time.strftime("%Y%m%d-%H%M%S")

root = os.path.expandvars(r"%APPDATA%\CodeBuddy CN\User")
dbs = [os.path.join(root, "globalStorage", "state.vscdb")]
for p in os.listdir(os.path.join(root, "workspaceStorage")):
    db = os.path.join(root, "workspaceStorage", p, "state.vscdb")
    if os.path.exists(db):
        dbs.append(db)

total_changed = 0
for db_path in dbs:
    tag = "GLOBAL" if "globalStorage" in db_path else os.path.basename(os.path.dirname(db_path))
    try:
        rows = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True).execute(
            "SELECT key, value FROM ItemTable").fetchall()
    except Exception as e:  # noqa: BLE001
        print("SKIP", tag, e)
        continue

    pending = []
    for k, v in rows:
        if k != "Tencent-Cloud.coding-copilot":
            continue
        try:
            obj = json.loads(str(v))
        except Exception:  # noqa: BLE001
            continue
        dirty = False
        for f in FIELDS:
            raw = obj.get(f)
            if not isinstance(raw, str):
                continue
            try:
                inner = json.loads(raw)
            except Exception:  # noqa: BLE001
                continue
            new = {kk: vv for kk, vv in inner.items() if not kk.startswith(PREFIX)}
            if len(new) != len(inner):
                print(f"  [{tag}] {f}: 删除 {len(inner) - len(new)} 项 -> {sorted(new)}")
                obj[f] = json.dumps(new, ensure_ascii=False, separators=(",", ":"))
                dirty = True
        sel = obj.get("chatSelectedModelMapV2")
        if isinstance(sel, str):
            try:
                sm = json.loads(sel)
            except Exception:  # noqa: BLE001
                sm = {}
            hit = [m for m, mid in sm.items() if isinstance(mid, str) and mid.startswith(PREFIX)]
            if hit:
                for m in hit:
                    sm[m] = FALLBACK
                print(f"  [{tag}] chatSelectedModelMapV2: {hit} -> {FALLBACK}")
                obj["chatSelectedModelMapV2"] = json.dumps(sm, ensure_ascii=False, separators=(",", ":"))
                dirty = True
        if dirty:
            pending.append((k, json.dumps(obj, ensure_ascii=False, separators=(",", ":"))))

    if not pending:
        continue

    bak = f"{db_path}.bak-clean-{STAMP}"
    try:
        shutil.copy2(db_path, bak)
        print(f"  备份 -> {os.path.basename(bak)}")
    except Exception as e:  # noqa: BLE001
        print("  备份失败，跳过该库:", e)
        continue

    try:
        db = sqlite3.connect(db_path, timeout=15)
        for k, val in pending:
            db.execute("UPDATE ItemTable SET value=? WHERE key=?", (val, k))
        db.commit()
        total_changed += len(pending)
        print(f"  [{tag}] 已写入 {len(pending)} 条")
        db.close()
    except Exception as e:  # noqa: BLE001
        print(f"  [{tag}] 写入失败（IDE 可能占用）:", e)

print("\n完成，改动条目:", total_changed)
print("提示：CodeBuddy 运行中可能回写旧状态，请重启 IDE 后生效。")
