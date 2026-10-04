#!/bin/bash
# macOS 原生构建入口：共用 Rust/Swift 源码，按当前机器架构生成完整 .app、ZIP 和校验文件。
# 任一编译、复制或签名步骤失败立即退出，避免把残缺应用当成成功产物。
set -euo pipefail

# 输入：无；返回：项目根目录及本机构架，所有输出保持在工作区内。
prepare() {
    PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)" # 脚本所在项目根目录，与启动命令的工作目录无关。
    APP_STAGE="$PROJECT_DIR/target/macos-app/Grid Planner.app" # 完成签名检查前使用的临时应用目录。
    APP_OUTPUT="$PROJECT_DIR/dist/Grid Planner.app" # 用户可以整体移动和双击的最终应用包。
    APP_ARCH="$(uname -m)" # 原生 CPU 架构；GitHub 为 arm64/x86_64 分别选择构建机器。
    ARCHIVE_NAME="GridPlanner-macOS-$APP_ARCH.zip" # 下载文件名标明架构，避免选错安装包。
    export MACOSX_DEPLOYMENT_TARGET=13.0 # Rust 与 Swift 使用相同最低系统目标，不依赖构建机当前版本。
    export CARGO_TARGET_DIR="$PROJECT_DIR/target" # 与后续复制路径一致，不受个人全局编译目录影响。
    mkdir -p "$APP_STAGE/Contents/MacOS" "$APP_STAGE/Contents/Resources" "$PROJECT_DIR/dist"
    cd "$PROJECT_DIR"
}

# 输入：项目目录；返回：已编译的 Rust 后端，可使用 GRID_BUILD_OFFLINE=1 离线构建。
build_engine() {
    # 离线模式只复用已缓存依赖；GitHub 首次构建使用联网模式，但始终遵守 Cargo.lock。
    if [ "${GRID_BUILD_OFFLINE:-0}" = "1" ]; then
        cargo build --release --locked --offline
    else
        cargo build --release --locked
    fi
    cp "$PROJECT_DIR/target/release/grid-planner" "$APP_STAGE/Contents/Resources/grid-planner"
}

# 输入：Swift 源文件与当前 CPU 架构；返回：最低 macOS 13 的原生菜单栏可执行文件。
build_desktop() {
    # Swift 6 开启并发检查；同机原生编译避免错把 Intel/Apple Silicon 二进制混装。
    xcrun swiftc -O -swift-version 6 -parse-as-library \
        -target "$APP_ARCH-apple-macos13.0" \
        -module-cache-path "$PROJECT_DIR/target/swift-module-cache" \
        "$PROJECT_DIR"/desktop/macos/*.swift \
        -o "$APP_STAGE/Contents/MacOS/GridPlanner"
    cp "$PROJECT_DIR/desktop/macos/Info.plist" "$APP_STAGE/Contents/Info.plist"
    # 把使用说明和示例作为应用资源打包，运行机器不需要保留源码仓库。
    cp "$PROJECT_DIR/README.md" "$APP_STAGE/Contents/Resources/README.md"
    cp -R "$PROJECT_DIR/docs" "$PROJECT_DIR/examples" "$APP_STAGE/Contents/Resources/"
    plutil -lint "$APP_STAGE/Contents/Info.plist"
}

# 输入：构建完成的临时 .app；返回：本地签名应用、ZIP 及 SHA-256 文件；失败退出非零。
# ad-hoc 签名验证包内容完整性，不是 Developer ID 签名，也不包含苹果公证。
package_app() {
    codesign --force --sign - "$APP_STAGE/Contents/Resources/grid-planner"
    codesign --force --sign - "$APP_STAGE"
    codesign --verify --deep --strict "$APP_STAGE"
    if [ -d "$APP_OUTPUT" ]; then
        # 当前 dist 产物保留一份到构建缓存，不删除用户其他目录的已安装应用。
        rm -rf "$PROJECT_DIR/target/macos-app/previous.app"
        mv "$APP_OUTPUT" "$PROJECT_DIR/target/macos-app/previous.app"
    fi
    mv "$APP_STAGE" "$APP_OUTPUT"
    # ditto 保留应用目录、可执行权限及 macOS 资源信息；不能直接逐文件上传 .app。
    ditto -c -k --sequesterRsrc --keepParent "$APP_OUTPUT" "$PROJECT_DIR/dist/$ARCHIVE_NAME"
    # 校验文件只写 ZIP 文件名，用户下载到任意目录后都可执行 shasum -c。
    (cd "$PROJECT_DIR/dist" && shasum -a 256 "$ARCHIVE_NAME" > "$ARCHIVE_NAME.sha256")
    printf '应用已构建：%s\n' "$APP_OUTPUT"
}

# 依次准备、编译、签名和压缩，后一步只消费前一步成功生成的内容。
prepare
build_engine
build_desktop
package_app
