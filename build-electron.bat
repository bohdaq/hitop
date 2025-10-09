@echo off
REM Build HITOP Electron App for Windows

echo.
echo ========================================
echo   HITOP Electron App Builder
echo ========================================
echo.

REM Step 1: Check if Node.js is installed
where node >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Node.js is not installed!
    echo Please install Node.js from https://nodejs.org/
    pause
    exit /b 1
)

echo Node.js version:
node --version
echo.

REM Step 2: Install dependencies if needed
if not exist "node_modules" (
    echo Installing dependencies...
    call npm install
    if %ERRORLEVEL% NEQ 0 (
        echo ERROR: Failed to install dependencies
        pause
        exit /b 1
    )
) else (
    echo Dependencies already installed
)
echo.

REM Step 3: Build React app
echo Building React application...
cd frontend
call npm run build
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Failed to build React app
    cd ..
    pause
    exit /b 1
)
cd ..
echo React build complete!
echo.

REM Step 4: Build Electron app
echo Building Electron application for Windows...
call npx electron-builder --win
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Failed to build Electron app
    pause
    exit /b 1
)
echo.

REM Step 5: Show results
echo ========================================
echo   Build Complete!
echo ========================================
echo.
echo Output directory: dist\
echo.
echo Built files:
dir dist\*.exe /b
echo.

REM Step 6: Show file sizes
echo File sizes:
for %%F in (dist\*.exe) do (
    echo   %%~nxF - %%~zF bytes
)
echo.

echo ========================================
echo   HITOP Electron app is ready!
echo ========================================
echo.
echo To test the app:
echo   1. Installer: dist\HITOP Setup 1.0.0.exe
echo   2. Portable: dist\HITOP 1.0.0.exe
echo.

pause
