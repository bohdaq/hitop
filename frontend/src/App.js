import './App.css';
import { useState, useEffect } from 'react';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import hljs from 'highlight.js';
import 'highlight.js/styles/github-dark.css';
import styles from './App.css'

function App() {
  const [url, setUrl] = useState('');
  const [loading, setLoading] = useState(false);
  const [response, setResponse] = useState(null);
  const [responseType, setResponseType] = useState('');

  const handleMakeRequest = async () => {
    if (!url) {
      alert('Please enter a URL');
      return;
    }

    setLoading(true);
    setResponse(null);
    try {
      const res = await fetch(url);
      const contentType = res.headers.get('content-type');
      let data;
      let type = 'text';

      if (contentType && contentType.includes('application/json')) {
        data = await res.json();
        data = JSON.stringify(data, null, 2);
        type = 'json';
      } else if (contentType && contentType.includes('text/html')) {
        data = await res.text();
        type = 'html';
      } else if (contentType && contentType.includes('text/xml')) {
        data = await res.text();
        type = 'xml';
      } else {
        data = await res.text();
        type = 'text';
      }

      setResponse(data);
      setResponseType(type);
      console.log('Response:', data);
    } catch (error) {
      console.error('Error:', error);
      setResponse(`Error: ${error.message}`);
      setResponseType('text');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (response) {
      document.querySelectorAll('pre code').forEach((block) => {
        hljs.highlightElement(block);
      });
    }
  }, [response]);

  return (
    <div className="App">
      <div className="AppContainer">
        <div className="ControlsContainer">
          <div className='UrlContainer'>
            <TextField
              fullWidth
              id="outlined-basic"
              label="URL"
              variant="outlined"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
            />
          </div>
          <div className='ButtonContainer'>
            <Button
              size='large'
              variant="contained"
              onClick={handleMakeRequest}
              disabled={loading}
            >
              {loading ? 'Loading...' : 'Make Request'}
            </Button>
          </div>
        </div>
        {response && (
          <div className="ResponseViewer">
            <h3>Response:</h3>
            <pre>
              <code className={`language-${responseType}`}>
                {response}
              </code>
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}

export default App;
