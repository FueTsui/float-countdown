@echo off
chcp 65001 >nul
cd /d "%~dp0"
where cargo >nul 2>nul || (echo 请先安装 Rust: https://www.rust-lang.org/tools/install & pause & exit /b 1)
where npm >nul 2>nul || (echo 请先安装 Node.js: https://nodejs.org/ & pause & exit /b 1)
if not exist node_modules\@tauri-apps\cli call npm install
call npm run build
echo.
echo 安装包在 src-tauri\target\release\bundle\nsis 文件夹里
explorer src-tauri\target\release\bundle\nsis
pause
