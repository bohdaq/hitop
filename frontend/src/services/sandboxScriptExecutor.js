/**
 * Sandboxed Script Executor
 * 
 * Executes scripts in a sandboxed iframe to comply with Chrome extension CSP
 */

class SandboxScriptExecutor {
  constructor() {
    this.iframe = null;
    this.ready = false;
    this.pendingCallbacks = new Map();
    this.callbackId = 0;
    this.initPromise = null;
    this.messageHandler = this.handleMessage.bind(this);
  }

  handleMessage(event) {
    if (event.data.type === 'sandboxReady') {
      this.ready = true;
      if (this.readyResolve) {
        this.readyResolve();
        this.readyResolve = null;
      }
    } else if (event.data.type === 'scriptResult') {
      const callback = this.pendingCallbacks.get(event.data.callbackId);
      if (callback) {
        this.pendingCallbacks.delete(event.data.callbackId);
        if (event.data.success) {
          callback.resolve(event.data.result);
        } else {
          callback.reject(new Error(event.data.error));
        }
      }
    }
  }

  async init() {
    if (this.ready) return;
    if (this.initPromise) return this.initPromise;

    this.initPromise = new Promise((resolve) => {
      this.readyResolve = resolve;
      
      // Create sandboxed iframe
      this.iframe = document.createElement('iframe');
      this.iframe.sandbox = 'allow-scripts';
      
      // Check if we're in a Chrome extension
      // eslint-disable-next-line no-undef
      const isExtension = typeof chrome !== 'undefined' && chrome.runtime && chrome.runtime.id;
      
      if (isExtension) {
        // In Chrome extension, use chrome.runtime.getURL
        // eslint-disable-next-line no-undef
        this.iframe.src = chrome.runtime.getURL('app/sandbox.html');
      } else {
        // In web version, use relative path
        this.iframe.src = `${window.location.origin}/sandbox.html`;
      }
      
      this.iframe.style.display = 'none';
      
      console.log('Sandbox iframe src:', this.iframe.src);
      
      // Listen for messages from sandbox
      window.addEventListener('message', this.messageHandler);

      // Wait for DOM to be ready
      if (document.body) {
        document.body.appendChild(this.iframe);
      } else {
        document.addEventListener('DOMContentLoaded', () => {
          document.body.appendChild(this.iframe);
        });
      }
    });

    return this.initPromise;
  }

  async executeScript(script, context) {
    try {
      await this.init();

      // Wait for iframe to be fully loaded with timeout
      const maxWaitTime = 5000; // 5 seconds max
      const startTime = Date.now();
      while (!this.ready && (Date.now() - startTime) < maxWaitTime) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }

      if (!this.ready) {
        throw new Error('Sandbox iframe failed to initialize');
      }

      return new Promise((resolve, reject) => {
        const callbackId = this.callbackId++;
        
        this.pendingCallbacks.set(callbackId, { resolve, reject });

        // Send script to sandbox with only serializable data
        if (this.iframe && this.iframe.contentWindow) {
          try {
            // Extract only serializable data from context
            const contextData = {
              url: context.url,
              method: context.method,
              headers: context.headers,
              body: context.body,
              variables: context.variables,
              context: context.context,
              response: context.response,
              responseText: context.responseText,
              responseHeaders: context.responseHeaders,
              statusCode: context.statusCode
            };
            
            this.iframe.contentWindow.postMessage({
              type: 'executeScript',
              callbackId,
              script,
              contextData
            }, '*');
          } catch (error) {
            this.pendingCallbacks.delete(callbackId);
            reject(new Error(`Failed to send message to sandbox: ${error.message}`));
            return;
          }
        } else {
          this.pendingCallbacks.delete(callbackId);
          reject(new Error('Sandbox iframe not ready'));
          return;
        }

        // Timeout after 10 seconds
        setTimeout(() => {
          if (this.pendingCallbacks.has(callbackId)) {
            this.pendingCallbacks.delete(callbackId);
            reject(new Error('Script execution timeout (10s)'));
          }
        }, 10000);
      });
    } catch (error) {
      console.error('Sandbox execution error:', error);
      throw error;
    }
  }

  destroy() {
    if (this.iframe && this.iframe.parentNode) {
      this.iframe.parentNode.removeChild(this.iframe);
    }
    window.removeEventListener('message', this.messageHandler);
    this.iframe = null;
    this.ready = false;
    this.initPromise = null;
  }
}

// Create singleton instance
const sandboxExecutor = new SandboxScriptExecutor();

export default sandboxExecutor;
