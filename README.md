# 悬浮倒计时 2.0(Tauri 版)

这是原 Electron 版「悬浮倒计时」的重写版本:主程序用 **Rust(Tauri 2)** 编写,界面用 HTML/CSS/JS。
原 Electron 版仍保留在 `../悬浮倒计时/`,两者的设置互不影响。

## 相比 1.0 的变化

- **体积更小**:不再打包 Chromium,安装包约 5 MB(原来约 100 MB),内存占用也更低。
- **演示联动改用 Rust 实现**:不再启动 PowerShell 脚本,检测更快更省电。
- **拖拽添加演示文件**:把 PPT / PDF 等文件拖到设置窗口(任意位置)或悬浮窗上即可添加。
- **选择文件夹自动识别**:「演示文件」页点「选择文件夹…」,或直接拖入文件夹,会自动找出里面所有演示文件
  (pptx / ppt / pps / ppsx / pdf / key / odp / dps 等,可选是否包含子文件夹,自动跳过 `~$` 临时文件)。
- **进度条开关**:「标题与进度条」页单独开关,还可以设置位置(数字上方/下方)和粗细;悬浮窗右键菜单和托盘菜单也能直接切换。
- **自定义标题**:在倒计时数字上方显示一行自定义文字,可调大小和颜色;右键菜单可快速开关。
- **原生风格界面**:
  - Windows 11:Mica 云母背景、Segoe UI Variable 字体、Fluent 风格的开关/滑块/按钮/卡片,跟随系统强调色与深浅色。Windows 10 自动退回纯色背景。
  - macOS:侧边栏毛玻璃、红绿灯按钮融入窗口、苹方/SF 字体、系统设置风格的分组列表与开关,跟随系统强调色与深浅色。
- **每个演示文件单独计时**:文件列表里点「专属计时」,放映哪个文件就自动用哪套环节(如题库每页 8 秒)。
- **超时计时**(到时后显示 -00:12)、向前/向后翻页分别设置、监视文件夹(新增的演示文件自动加入)、放映时悬浮窗自动跟到放映所在的屏幕。
- **设置界面**:侧栏可折叠,演示文件页置顶,默认字体 Bahnschrift(下拉选择),外观页有实时预览。
- 首次运行会自动打开设置窗口,并沿用旧版的设置(环节、外观、已绑定文件、快捷键)。

## 目录结构

```
src/                前端页面
  widget.html/js    悬浮窗(计时逻辑与显示)
  settings.*        设置窗口
  common.js         和主程序通信、处理拖入的文件
  parse.js          解析计时配置(“题库 8秒”这类文本)
src-tauri/          Rust 主程序
  src/lib.rs        窗口、托盘、快捷键、命令入口
  src/settings.rs   设置的默认值、读写与备份
  src/files.rs      演示文件/文件夹/配置的识别与导入导出
  src/link.rs       演示联动(Windows:前台窗口检测、翻页按键、自动翻页)
  tauri.conf.json   应用配置(名称、图标、安装包)
```

## 自己编译

需要先安装:
1. [Rust](https://www.rust-lang.org/tools/install)(Windows 上安装时按提示装好 “Visual Studio C++ 生成工具”)
2. [Node.js](https://nodejs.org/) 18 或更高版本
3. macOS 还需要 Xcode 命令行工具:`xcode-select --install`

然后在本文件夹里:

```bash
npm install          # 第一次需要
npm run dev          # 开发模式运行
npm test             # 运行前端测试(Rust 测试用 cargo test)
npm run build        # 生成安装包
```

安装包位置:
- Windows:`src-tauri/target/release/bundle/nsis/悬浮倒计时_2.1.0_x64-setup.exe`
- macOS:`src-tauri/target/release/bundle/dmg/悬浮倒计时_2.1.0_*.dmg`

Windows 上也可以直接双击 `构建安装包.bat`。

## 已知限制

- **演示联动只支持 Windows**。macOS 上可以添加演示文件,但不会自动检测放映和翻页(需要额外的辅助功能权限,后续可以再做)。
- macOS 版没有在真机上测试过,首次打开如果提示“无法验证开发者”,请在「系统设置 → 隐私与安全性」里点「仍要打开」。
- 设置保存在 `%APPDATA%\com.dario.floatcountdown\settings.json`(macOS 在 `~/Library/Application Support/com.dario.floatcountdown/`)。第一次运行时会自动沿用旧版的设置。
