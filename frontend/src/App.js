import './App.css';
import { useState, useEffect } from 'react';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import Select from '@mui/material/Select';
import MenuItem from '@mui/material/MenuItem';
import FormControl from '@mui/material/FormControl';
import InputLabel from '@mui/material/InputLabel';
import hljs from 'highlight.js';
import 'highlight.js/styles/github-dark.css';
import styles from './App.css'

function App() {
  const [url, setUrl] = useState('');
  const [method, setMethod] = useState('GET');
  const [loading, setLoading] = useState(false);
  const [response, setResponse] = useState(null);
  const [responseType, setResponseType] = useState('');
  const [headers, setHeaders] = useState([{ name: '', value: '' }]);
  const [responseHeaders, setResponseHeaders] = useState(null);
  const [requestBody, setRequestBody] = useState('');

  const addHeader = () => {
    setHeaders([...headers, { name: '', value: '' }]);
  };

  const updateHeader = (index, field, value) => {
    const newHeaders = [...headers];
    newHeaders[index][field] = value;
    setHeaders(newHeaders);
  };

  const removeHeader = (index) => {
    const newHeaders = headers.filter((_, i) => i !== index);
    setHeaders(newHeaders.length > 0 ? newHeaders : [{ name: '', value: '' }]);
  };

  const handleMakeRequest = async () => {
    if (!url) {
      alert('Please enter a URL');
      return;
    }

    setLoading(true);
    setResponse(null);
    try {
      // Build headers object from the headers array
      const requestHeaders = {};
      headers.forEach(header => {
        if (header.name && header.value) {
          requestHeaders[header.name] = header.value;
        }
      });

      const fetchOptions = {
        method: method,
        headers: requestHeaders
      };

      // Add body to request if it exists and method supports it
      if (requestBody && (method === 'POST' || method === 'PUT' || method === 'PATCH')) {
        fetchOptions.body = requestBody;
      }

      const res = await fetch(url, fetchOptions);
      
      // Extract response headers
      const resHeaders = {};
      res.headers.forEach((value, key) => {
        resHeaders[key] = value;
      });
      setResponseHeaders(resHeaders);

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
      setResponseHeaders(null);
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
          <FormControl className='MethodSelect'>
            <InputLabel id="method-select-label">Method</InputLabel>
            <Select
              labelId="method-select-label"
              id="method-select"
              value={method}
              label="Method"
              onChange={(e) => setMethod(e.target.value)}
            >
              <MenuItem value="GET">GET</MenuItem>
              <MenuItem value="POST">POST</MenuItem>
              <MenuItem value="PUT">PUT</MenuItem>
              <MenuItem value="PATCH">PATCH</MenuItem>
              <MenuItem value="DELETE">DELETE</MenuItem>
              <MenuItem value="HEAD">HEAD</MenuItem>
              <MenuItem value="OPTIONS">OPTIONS</MenuItem>
            </Select>
          </FormControl>
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
        <div className="HeadersSection">
          <h3>Headers</h3>
          {headers.map((header, index) => (
            <div key={index} className="HeaderRow">
              <TextField
                label="Header Name"
                variant="outlined"
                size="small"
                value={header.name}
                onChange={(e) => updateHeader(index, 'name', e.target.value)}
                className="HeaderInput"
              />
              <TextField
                label="Header Value"
                variant="outlined"
                size="small"
                value={header.value}
                onChange={(e) => updateHeader(index, 'value', e.target.value)}
                className="HeaderInput"
              />
              {headers.length > 1 && (
                <Button
                  variant="outlined"
                  color="error"
                  size="small"
                  onClick={() => removeHeader(index)}
                >
                  Remove
                </Button>
              )}
            </div>
          ))}
          <Button
            variant="outlined"
            onClick={addHeader}
            className="AddHeaderButton"
          >
            Add Header
          </Button>
        </div>
        {(method === 'POST' || method === 'PUT' || method === 'PATCH') && (
          <div className="BodySection">
            <h3>Body</h3>
            <TextField
              fullWidth
              multiline
              rows={8}
              variant="outlined"
              placeholder="Enter request body here..."
              value={requestBody}
              onChange={(e) => setRequestBody(e.target.value)}
            />
          </div>
        )}
        {response && (
          <div className="ResponseViewer">
            <h3>Response:</h3>
            {responseHeaders && (
              <div className="ResponseHeadersSection">
                <h4>Headers</h4>
                <div className="ResponseHeadersList">
                  {Object.entries(responseHeaders).map(([key, value]) => (
                    <div key={key} className="ResponseHeaderItem">
                      <span className="ResponseHeaderKey">{key}:</span>
                      <span className="ResponseHeaderValue">{value}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
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
