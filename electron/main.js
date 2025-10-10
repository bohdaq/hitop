const { app, BrowserWindow, ipcMain, session } = require('electron');
const path = require('path');
const fs = require('fs');

let mainWindow;

// Windows compatibility - only disable problematic features
if (process.platform === 'win32') {
  app.commandLine.appendSwitch('no-sandbox');
  app.commandLine.appendSwitch('disable-gpu-sandbox');
}

// User data path for storing app data
const userDataPath = app.getPath('userData');
const storageFilePath = path.join(userDataPath, 'hitop-data.json');

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 800,
    minHeight: 600,
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: path.join(__dirname, 'preload.js'),
      webSecurity: true
    },
    icon: path.join(__dirname, '../frontend/public/logo512.png'),
    title: 'HITOP - HTTP API Testing Tool',
    backgroundColor: '#ffffff'
  });

  // Load the React app
  const startUrl = process.env.ELECTRON_START_URL || 
    `file://${path.join(__dirname, '../frontend/build/index.html').replace(/\\/g, '/')}`;
  
  console.log('Loading URL:', startUrl);
  console.log('Build path:', path.join(__dirname, '../frontend/build'));
  
  mainWindow.loadURL(startUrl).catch(err => {
    console.error('Failed to load URL:', err);
  });

  // Log when page finishes loading
  mainWindow.webContents.on('did-finish-load', () => {
    console.log('Page loaded successfully');
  });

  // Log any console messages from renderer
  mainWindow.webContents.on('console-message', (event, level, message, line, sourceId) => {
    console.log(`Renderer console [${level}]:`, message);
  });

  // Open DevTools in development or on error
  if (process.env.NODE_ENV === 'development') {
    mainWindow.webContents.openDevTools();
  }

  // Open DevTools on crash
  mainWindow.webContents.on('crashed', () => {
    console.error('Renderer process crashed');
    mainWindow.webContents.openDevTools();
  });

  // Handle window close
  mainWindow.on('closed', () => {
    mainWindow = null;
  });

  // Set up CSP to allow custom scripts
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': ["default-src 'self' 'unsafe-inline' 'unsafe-eval'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'"]
      }
    });
  });

  // Handle load failures
  mainWindow.webContents.on('did-fail-load', (event, errorCode, errorDescription) => {
    console.error('Failed to load:', errorCode, errorDescription);
  });
}

// App lifecycle
app.whenReady().then(() => {
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// IPC Handlers for storage
ipcMain.handle('storage-get', async (event, key) => {
  try {
    if (fs.existsSync(storageFilePath)) {
      const data = JSON.parse(fs.readFileSync(storageFilePath, 'utf8'));
      return data[key] || null;
    }
    return null;
  } catch (error) {
    console.error('Storage get error:', error);
    return null;
  }
});

ipcMain.handle('storage-set', async (event, key, value) => {
  try {
    let data = {};
    if (fs.existsSync(storageFilePath)) {
      data = JSON.parse(fs.readFileSync(storageFilePath, 'utf8'));
    }
    data[key] = value;
    fs.writeFileSync(storageFilePath, JSON.stringify(data, null, 2));
    return true;
  } catch (error) {
    console.error('Storage set error:', error);
    return false;
  }
});

ipcMain.handle('storage-remove', async (event, key) => {
  try {
    if (fs.existsSync(storageFilePath)) {
      const data = JSON.parse(fs.readFileSync(storageFilePath, 'utf8'));
      delete data[key];
      fs.writeFileSync(storageFilePath, JSON.stringify(data, null, 2));
    }
    return true;
  } catch (error) {
    console.error('Storage remove error:', error);
    return false;
  }
});

ipcMain.handle('storage-clear', async () => {
  try {
    if (fs.existsSync(storageFilePath)) {
      fs.unlinkSync(storageFilePath);
    }
    return true;
  } catch (error) {
    console.error('Storage clear error:', error);
    return false;
  }
});

// HTTP request handler (bypasses CORS)
ipcMain.handle('http-request', async (event, options) => {
  const https = require('https');
  const http = require('http');
  
  return new Promise((resolve, reject) => {
    const url = new URL(options.url);
    const protocol = url.protocol === 'https:' ? https : http;
    
    const requestOptions = {
      hostname: url.hostname,
      port: url.port,
      path: url.pathname + url.search,
      method: options.method || 'GET',
      headers: options.headers || {}
    };

    const req = protocol.request(requestOptions, (res) => {
      let data = '';
      
      res.on('data', (chunk) => {
        data += chunk;
      });
      
      res.on('end', () => {
        resolve({
          status: res.statusCode,
          statusText: res.statusMessage,
          headers: res.headers,
          data: data
        });
      });
    });

    req.on('error', (error) => {
      reject(error);
    });

    if (options.body) {
      req.write(options.body);
    }

    req.end();
  });
});

console.log('HITOP Electron app started');
console.log('User data path:', userDataPath);
