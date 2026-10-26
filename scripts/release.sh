#!/usr/bin/env bash
# GreenPool 一键发版：bump → 提交 → 打 tag → 等 GitHub Actions → 同步镜像 → 双通道校验
#
# 用法（Git Bash）：
#   bash scripts/release.sh 0.3.8 "本次改动说明"
#
# 可选环境变量：
#   SKIP_CHECK=1   跳过 typecheck / cargo check（着急时用）
#   HTTPS_PROXY    访问 GitHub 的代理，默认 http://127.0.0.1:7890
#
# 前提：
#   - 当前在 main、工作区干净
#   - 仓库 secret TAURI_SIGNING_PRIVATE_KEY 已配置（CI 用）
#   - 本机可 ssh hk-server（镜像同步）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VER="${1:-}"
NOTES="${2:-见提交历史}"
REPO="nstl250619-ctrl/gp-app"
PROXY="${HTTPS_PROXY:-http://127.0.0.1:7890}"
SKIP_CHECK="${SKIP_CHECK:-0}"
TAG="v${VER}"

die() { echo "!! $*" >&2; exit 1; }

[[ -n "$VER" ]] || die "用法: bash scripts/release.sh <x.y.z> [说明]"
[[ "$VER" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "版本号格式应为 x.y.z（例：0.3.8）"

echo "== 1/7 前置检查 =="
BR="$(git rev-parse --abbrev-ref HEAD)"
[[ "$BR" == "main" ]] || die "当前分支 $BR，需切到 main"
if [[ -n "$(git status --porcelain)" ]]; then
  echo "工作区有未提交改动："
  git status --short
  die "请先提交或 stash"
fi
git rev-parse "$TAG" >/dev/null 2>&1 && die "本地已存在 tag $TAG"
[[ -f .tauri/update.key ]] || die "缺少 .tauri/update.key（本地签名用，CI 用 secret）"

echo "== 2/7 改版本号 -> $VER =="
sed -i -E "s/(\"version\": \")[0-9]+\.[0-9]+\.[0-9]+(\",)/\1${VER}\2/" src-tauri/tauri.conf.json
sed -i -E "0,/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/s//version = \"${VER}\"/" src-tauri/Cargo.toml
grep -m1 '"version"' src-tauri/tauri.conf.json
grep -m1 '^version = ' src-tauri/Cargo.toml

if [[ "$SKIP_CHECK" != "1" ]]; then
  echo "== 3/7 类型检查 =="
  npm run typecheck
  echo "== 4/7 Rust 检查（会顺带更新 Cargo.lock）=="
  ( cd src-tauri && cargo check --message-format short 2>&1 | tail -3 )
else
  echo "== 3-4/7 跳过检查（SKIP_CHECK=1）=="
fi

echo "== 5/7 提交并推送 =="
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore(release): v${VER} - ${NOTES}"
git push origin main

echo "== 6/7 打 tag 触发 CI: $TAG =="
git tag "$TAG"
git push origin "$TAG"

TOKEN="$(git credential fill <<< $'protocol=https\nhost=github.com\n\n' | sed -n 's/^password=//p')"
[[ -n "$TOKEN" ]] || die "拿不到 GitHub token（git credential）"
SHA="$(git rev-parse HEAD)"

gh_api() {
  curl -s -x "$PROXY" -H "Authorization: token $TOKEN" \
       -H "Accept: application/vnd.github+json" -H "User-Agent: gp-release" "$1"
}

echo "   等待 CI（Windows + macOS 并行，通常 8-12 分钟）…"
CONCL=""
for i in $(seq 1 60); do
  sleep 20
  RUN="$(gh_api "https://api.github.com/repos/${REPO}/actions/runs?head_sha=${SHA}&per_page=1")"
  LINE="$(printf '%s' "$RUN" | python -c "
import json,sys
d=json.load(sys.stdin)
r=(d.get('workflow_runs') or [None])[0]
print(('%s|%s' % (r['status'], r['conclusion'])) if r else 'none|none')
" 2>/dev/null || echo 'parse|error')"
  ST="${LINE%%|*}"; CONCL="${LINE##*|}"
  echo "   [$i] status=$ST conclusion=$CONCL"
  [[ "$ST" == "completed" ]] && break
done
[[ "$CONCL" == "success" ]] || die "CI 未成功（conclusion=$CONCL），请到 Actions 查看日志"

echo "== 7/7 校验产物 + 同步镜像 =="
LATEST="$(gh_api "https://api.github.com/repos/${REPO}/releases/tags/${TAG}" | python -c "
import json,sys
d=json.load(sys.stdin)
print(' '.join(sorted(a['name'] for a in d.get('assets',[]))))
")"
echo "   release 资产: $LATEST"
for need in latest.json "GreenPool_${VER}_x64-setup.exe" "GreenPool_aarch64.app.tar.gz"; do
  case " $LATEST " in *" $need "*) ;; *) die "缺少资产 $need" ;; esac
done

ssh -o BatchMode=yes hk-server "bash /opt/gp-mirror-sync.sh ${VER}" | tail -8

echo "   双通道校验："
curl -s -x "$PROXY" "https://github.com/${REPO}/releases/download/${TAG}/latest.json" \
  | python -c "import json,sys;d=json.load(sys.stdin);print('   GitHub  version=%s platforms=%s' % (d['version'], ','.join(d['platforms'])))"
curl -s "https://aigw.greenpool.cn/downloads/latest.json" \
  | python -c "import json,sys;d=json.load(sys.stdin);print('   aigw    version=%s platforms=%s' % (d['version'], ','.join(d['platforms'])))"

echo ""
echo "✅ 发版完成：$TAG"
echo "   https://github.com/${REPO}/releases/tag/${TAG}"
