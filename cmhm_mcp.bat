@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
if not defined CMHM_CONFIG (
    if exist "%SCRIPT_DIR%config.toml" set "CMHM_CONFIG=%SCRIPT_DIR%config.toml"
)
if exist "%SCRIPT_DIR%target\x86_64-pc-windows-gnu\release\cmhm_mcp.exe" (
    "%SCRIPT_DIR%target\x86_64-pc-windows-gnu\release\cmhm_mcp.exe"
) else (
    "%SCRIPT_DIR%target\x86_64-pc-windows-gnu\debug\cmhm_mcp.exe"
)
endlocal
