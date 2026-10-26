@echo off
rem GreenPool 一键发版（Windows 包装脚本）
rem 用法：scripts\release.cmd 0.3.8 "本次改动说明"
rem 说明：系统自带的 bash 是 WSL 占位，这里固定用 Git Bash。

setlocal
set "GITBASH=C:\Program Files\Git\bin\bash.exe"
if not exist "%GITBASH%" (
  echo [ERROR] 找不到 Git Bash: %GITBASH%
  echo         请安装 Git for Windows，或修改本脚本里的 GITBASH 路径。
  exit /b 1
)

"%GITBASH%" "%~dp0release.sh" %*
exit /b %ERRORLEVEL%
