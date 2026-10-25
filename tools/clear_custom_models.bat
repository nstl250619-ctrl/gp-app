@echo off
chcp 65001 >nul
setlocal
echo ============================================
echo  CodeBuddy 自定义模型 本地引用清理
echo ============================================
echo.

tasklist /FI "IMAGENAME eq CodeBuddy CN.exe" 2>nul | find /I "CodeBuddy CN.exe" >nul
if not errorlevel 1 (
    echo [停止] CodeBuddy 仍在运行。
    echo 请完全退出 CodeBuddy 后再双击本脚本。
    echo.
    pause
    exit /b 1
)

echo CodeBuddy 未运行，开始清理...
echo.
"C:\Python314\python.exe" "%~dp0_cb_clean.py"
echo.
echo 完成后重新打开 CodeBuddy，模型选择里就不再有 custom-local 残留。
pause
