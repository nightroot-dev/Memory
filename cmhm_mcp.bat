@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
if not defined CMHM_CONFIG (
    if exist "%SCRIPT_DIR%config.toml" set "CMHM_CONFIG=%SCRIPT_DIR%config.toml"
)
rem Pin the working directory to this folder: config.toml uses relative
rem paths (data_dir, projections_dir), so data\ and data\projections\ must
rem resolve inside this folder no matter where the MCP client launched us.
pushd "%SCRIPT_DIR%"
if exist "%SCRIPT_DIR%target\x86_64-pc-windows-gnu\release\cmhm_mcp.exe" (
    "%SCRIPT_DIR%target\x86_64-pc-windows-gnu\release\cmhm_mcp.exe"
) else (
    "%SCRIPT_DIR%target\x86_64-pc-windows-gnu\debug\cmhm_mcp.exe"
)
popd
endlocal
