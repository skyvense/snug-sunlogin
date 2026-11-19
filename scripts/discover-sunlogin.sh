#!/usr/bin/env bash
# discover-sunlogin.sh — 在装有向日葵（AweSun）的 Mac 上运行，抓取「Snug 向日葵版」定版所需的事实。
#
# Snug 判断「正在被远程」靠三路信号（见 docs/how-it-works.md），移植到向日葵要先确认：
#   1. 向日葵主程序的 bundle id 与可执行文件名（身份）
#   2. 被控时哪个进程在推流：系统日志里 ScreenCaptureKit / VideoToolbox 事件的来源进程
#      （Snug 的 CaptureLogParser 认的就是这几条框架日志，只是按进程名过滤）
#   3. 被控时向日葵在屏上放了哪些窗口（覆盖窗口信号）、哪个进程在注入输入（手势接管认它）
#
# 用法：
#   scripts/discover-sunlogin.sh            # 未连接时先跑一次（基线）
#   scripts/discover-sunlogin.sh --during   # 手机已连上、正在被控时跑（会采样 25 秒系统日志）
#
# 全程只读，不改任何设置。输出写到 ~/Desktop/snug-sunlogin-discovery-<时间>/report.txt，
# 把整个文件夹发回给开发方即可。
# 被 sh/zsh 误启动时自己切回 bash（进程替换等语法只有 bash 支持）
[ -z "${BASH_VERSION:-}" ] && exec bash "$0" "$@"

MODE="${1:-baseline}"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="$HOME/Desktop/snug-sunlogin-discovery-$STAMP"
mkdir -p "$OUT"
REPORT="$OUT/report.txt"
exec > >(tee "$REPORT") 2>&1

echo "=== Snug 向日葵版 实机发现报告（$MODE）==="
echo "时间：$(date)  主机：$(scutil --get ComputerName 2>/dev/null || hostname)  系统：$(sw_vers -productVersion)"

echo
echo "--- 1. 向日葵 App 本体 ---"
APP=""
for cand in /Applications/AweSun.app "$HOME/Applications/AweSun.app"; do
  [ -d "$cand" ] && APP="$cand" && break
done
if [ -z "$APP" ]; then
  APP="$(mdfind 'kMDItemCFBundleIdentifier == "com.oray.sunlogin.macclient"' 2>/dev/null | head -1)"
fi
if [ -n "$APP" ] && [ -d "$APP" ]; then
  echo "路径：$APP"
  echo "Bundle ID：$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist" 2>/dev/null)"
  echo "主可执行：$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist" 2>/dev/null)"
  echo "版本：$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist" 2>/dev/null)"
  echo "Contents/MacOS 下的可执行文件："
  ls -1 "$APP/Contents/MacOS" 2>/dev/null | sed 's/^/  /'
  echo "包内嵌套的 .app / .xpc（helper 们）："
  find "$APP/Contents" -maxdepth 4 \( -name '*.app' -o -name '*.xpc' \) 2>/dev/null | sed "s|$APP/||" | sed 's/^/  /'
else
  echo "未找到 AweSun.app（mdfind 也没有）——请确认向日葵装在这台 Mac 上"
fi

echo
echo
echo "--- 1b. 显示器清单（分辨率 / 旋转 / 主屏）---"
system_profiler SPDisplaysDataType 2>/dev/null | sed 's/^/  /'

echo "--- 2. 向日葵相关进程与后台项 ---"
echo "进程（pgrep awesun|sunlogin|oray）："
pgrep -il 'awesun|sunlogin|oray' 2>/dev/null | sed 's/^/  /' || echo "  （无）"
echo "当前用户的 launchd 项："
launchctl list 2>/dev/null | grep -iE 'oray|awesun|sunlogin' | sed 's/^/  /' || echo "  （无）"
echo "（系统域 launchd 项需要管理员权限，如实记录即可：sudo launchctl list | grep -i oray）"

echo
echo "--- 3. 当前在屏窗口（owner / 图层 / 名称）---"
if command -v swift >/dev/null 2>&1; then
  cat > "$OUT/windows.swift" <<'SWIFT'
import CoreGraphics
import Foundation
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
if let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] {
    for w in list {
        let owner = w[kCGWindowOwnerName as String] as? String ?? "?"
        let name = w[kCGWindowName as String] as? String ?? ""
        let layer = w[kCGWindowLayer as String] as? Int ?? 0
        print("owner=\(owner) layer=\(layer) name=\(name)")
    }
}
SWIFT
  swift "$OUT/windows.swift" 2>/dev/null | sort | uniq -c | sort -rn | sed 's/^/  /'
else
  echo "  未装 Xcode 命令行工具（swift 不可用），跳过窗口清单"
fi

if [ "$MODE" = "--during" ]; then
  echo
  echo "--- 4. 推流日志采样（25 秒，保持手机连接并滑动几下屏幕）---"
  PRED='eventMessage CONTAINS "SCDisplay initWithDict:" OR eventMessage CONTAINS "CaptureWithCompletionHandler:]" OR eventMessage CONTAINS "VTCompressionSession"'
  RAW="$OUT/capture-log.ndjson"
  /usr/bin/log stream --style ndjson --predicate "$PRED" > "$RAW" 2>/dev/null &
  LOGPID=$!
  sleep 25
  kill "$LOGPID" 2>/dev/null
  wait "$LOGPID" 2>/dev/null
  echo "采样到 $(wc -l < "$RAW" | tr -d ' ') 行。按来源进程汇总："
  /usr/bin/python3 - "$RAW" <<'PYEOF' | sed 's/^/  /'
import json, sys, collections
counts = collections.Counter()
samples = {}
for line in open(sys.argv[1], encoding='utf-8', errors='replace'):
    line = line.strip()
    if not line.startswith('{'):
        continue
    try:
        o = json.loads(line)
    except Exception:
        continue
    proc = o.get('processImagePath') or o.get('process') or '?'
    counts[proc] += 1
    samples.setdefault(proc, o.get('eventMessage', '')[:110])
for proc, n in counts.most_common():
    print(f"{n:5d}  {proc}")
    print(f"       例：{samples[proc]}")
if not counts:
    print("（一条都没采到：可能向日葵不用 ScreenCaptureKit 推流，或这 25 秒内没有开/停推流动作——")
    print("  请断开重连一次再跑 --during，让它打出开始/停止事件）")
PYEOF
  echo
  echo "--- 5. 最近 10 分钟的同类日志（log show 回看）---"
  /usr/bin/log show --last 10m --style ndjson --predicate "$PRED" 2>/dev/null \
    | /usr/bin/python3 -c '
import json, sys, collections
c = collections.Counter()
for line in sys.stdin:
    line = line.strip()
    if line.startswith("{"):
        try: c[json.loads(line).get("processImagePath", "?")] += 1
        except Exception: pass
for p, n in c.most_common(): print(f"  {n:5d}  {p}")
print("  （空）" if not c else "", end="")
'
fi

echo
echo "=== 报告结束。把 $OUT 整个文件夹发回即可 ==="
