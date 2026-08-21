@echo off
REM Nudge GUI Installer for Windows
REM Double-click to install

title Nudge Installer
color 0A

echo.
echo  ========================================
echo     Nudge Installer v1.2.0
echo  ========================================
echo.
echo  Installing Nudge programming language...
echo.

REM Check if running as admin
net session >nul 2>&1
if %errorLevel% == 0 (
    echo  Running as Administrator
) else (
    echo  Please run as Administrator
    echo  Right-click and select "Run as administrator"
    pause
    exit /b 1
)

REM Download URL
set VERSION=v1.2.0
set URL=https://github.com/NekomyaDev/nudge/releases/download/%VERSION%/nudgec-%VERSION%-windows-x86_64.zip
set INSTALL_DIR=C:\Program Files\Nudge
set TMP_DIR=%TEMP%\nudge_install

echo  Creating installation directory...
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"

echo  Downloading Nudge %VERSION%...
powershell -Command "Invoke-WebRequest -Uri '%URL%' -OutFile '%TMP_DIR%\nudge.zip'"

echo  Extracting...
powershell -Command "Expand-Archive -Path '%TMP_DIR%\nudge.zip' -DestinationPath '%TMP_DIR%' -Force"

echo  Installing...
copy "%TMP_DIR%\nudgec.exe" "%INSTALL_DIR%\nudgec.exe" >nul

echo  Adding to PATH...
setx PATH "%PATH%;%INSTALL_DIR%" /M >nul 2>&1

echo  Cleaning up...
rmdir /s /q "%TMP_DIR%" >nul 2>&1

echo.
echo  ========================================
echo     Installation Complete!
echo  ========================================
echo.
echo  Nudge has been installed to: %INSTALL_DIR%
echo.
echo  To use Nudge, open a new command prompt and type:
echo    nudgec --help
echo.
echo  Quick start:
echo    nudgec check hello.ndg
echo    nudgec build hello.ndg
echo    nudgec test hello.ndg
echo.
echo  VS Code Extension:
echo    https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang
echo.
pause
