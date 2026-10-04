#!/bin/bash
# 桌面桥接 E2E：复用生产表单/进程代码，调用已打包应用的真实 Rust 后端。
# 任一步失败即停止；此验收不点击 GUI，也不会保存用户的表单偏好。
set -euo pipefail
TEST_PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)" # 根据脚本位置定位源码根目录。
TEST_APP_ENGINE="$TEST_PROJECT_DIR/dist/Grid Planner.app/Contents/Resources/grid-planner" # 使用实际发货的包内后端。
mkdir -p "$TEST_PROJECT_DIR/target/desktop-tests"
if [ ! -x "$TEST_APP_ENGINE" ]; then
    printf '先运行 bash scripts/build-macos.sh 构建应用。\n' >&2
    exit 1
fi
# 独立编译验收入口，只复用生产 FormState/Engine，不替换算法或伪造后端输出。
xcrun swiftc -O -swift-version 6 -parse-as-library \
    -module-cache-path "$TEST_PROJECT_DIR/target/swift-module-cache" \
    "$TEST_PROJECT_DIR/desktop/macos/FormState.swift" \
    "$TEST_PROJECT_DIR/desktop/macos/Engine.swift" \
    "$TEST_PROJECT_DIR/desktop/tests/BridgeE2E.swift" \
    -o "$TEST_PROJECT_DIR/target/desktop-tests/BridgeE2E"
# Python 提供回环 HTTP 服务，再启动 Swift 验收；真实行情与账户密钥都不是验收前提。
python3 "$TEST_PROJECT_DIR/desktop/tests/http_fixture.py" \
    "$TEST_PROJECT_DIR/target/desktop-tests/BridgeE2E" "$TEST_APP_ENGINE"
