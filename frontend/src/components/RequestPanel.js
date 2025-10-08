import React from 'react';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import Select from '@mui/material/Select';
import MenuItem from '@mui/material/MenuItem';
import FormControl from '@mui/material/FormControl';
import InputLabel from '@mui/material/InputLabel';
import IconButton from '@mui/material/IconButton';
import SaveIcon from '@mui/icons-material/Save';

const RequestPanel = ({
  tabData,
  onUpdateTabData,
  onMakeRequest,
  onSaveRequest,
  onAddHeader,
  onUpdateHeader,
  onRemoveHeader,
  getStatusText
}) => {
  return (
    <div className="AppContainer">
      <div className="ControlsContainer">
        <FormControl className='MethodSelect'>
          <InputLabel id="method-select-label">Method</InputLabel>
          <Select
            labelId="method-select-label"
            id="method-select"
            value={tabData.method}
            label="Method"
            onChange={(e) => onUpdateTabData({ method: e.target.value })}
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
            value={tabData.url}
            onChange={(e) => onUpdateTabData({ url: e.target.value })}
          />
        </div>
        <div className='ButtonContainer'>
          <Button
            size='large'
            variant="contained"
            onClick={onMakeRequest}
            disabled={tabData.loading}
          >
            {tabData.loading ? 'Loading...' : 'Make Request'}
          </Button>
          <IconButton
            color="primary"
            onClick={onSaveRequest}
            aria-label="save request"
            disabled={!tabData.url}
          >
            <SaveIcon />
          </IconButton>
        </div>
      </div>
      <div className="HeadersSection">
        <h3>Headers</h3>
        {tabData.headers.map((header, index) => (
          <div key={index} className="HeaderRow">
            <TextField
              label="Header Name"
              variant="outlined"
              size="small"
              value={header.name}
              onChange={(e) => onUpdateHeader(index, 'name', e.target.value)}
              className="HeaderInput"
            />
            <TextField
              label="Header Value"
              variant="outlined"
              size="small"
              value={header.value}
              onChange={(e) => onUpdateHeader(index, 'value', e.target.value)}
              className="HeaderInput"
            />
            {tabData.headers.length > 1 && (
              <Button
                variant="outlined"
                color="error"
                size="small"
                onClick={() => onRemoveHeader(index)}
              >
                Remove
              </Button>
            )}
          </div>
        ))}
        <Button
          variant="outlined"
          onClick={onAddHeader}
          className="AddHeaderButton"
        >
          Add Header
        </Button>
      </div>
      {(tabData.method === 'POST' || tabData.method === 'PUT' || tabData.method === 'PATCH') && (
        <div className="BodySection">
          <h3>Body</h3>
          <TextField
            fullWidth
            multiline
            rows={8}
            variant="outlined"
            placeholder="Enter request body here..."
            value={tabData.requestBody}
            onChange={(e) => onUpdateTabData({ requestBody: e.target.value })}
          />
        </div>
      )}
      <div className="PreRequestScriptSection">
        <h3>Pre-Request Script</h3>
        <TextField
          fullWidth
          multiline
          rows={6}
          variant="outlined"
          placeholder="// JavaScript code to run before request&#10;// Available variables:&#10;// url, method, headers, body - Current request data&#10;// variables - Collection variables object&#10;// context - Runtime context object&#10;&#10;// Available functions:&#10;// setHeader(name, value) - Add/update header&#10;// setUrl(url) - Modify URL&#10;// setBody(body) - Modify request body&#10;// setContext(key, value) - Store data for next request&#10;// getContext(key) - Retrieve stored data&#10;// getVariable(key) - Get collection variable&#10;&#10;// Example 1: Use collection variables&#10;// const baseUrl = getVariable('apiUrl');&#10;// setUrl(baseUrl + '/users');&#10;&#10;// Example 2: Use context from previous request&#10;// const token = getContext('authToken');&#10;// setHeader('Authorization', 'Bearer ' + token);&#10;&#10;// Example 3: Dynamic URL with variable and context&#10;// const userId = getContext('userId') || getVariable('defaultUserId');&#10;// setUrl(getVariable('apiUrl') + '/users/' + userId);"
          value={tabData.preRequestScript}
          onChange={(e) => onUpdateTabData({ preRequestScript: e.target.value })}
          sx={{ fontFamily: 'monospace' }}
        />
      </div>
      <div className="PostRequestScriptSection">
        <h3>Post-Request Script</h3>
        <TextField
          fullWidth
          multiline
          rows={6}
          variant="outlined"
          placeholder="// JavaScript code to run after request completes&#10;// Available variables:&#10;// response - Parsed JSON response (or responseText if not JSON)&#10;// responseText - Raw response text&#10;// responseHeaders - Response headers object&#10;// statusCode - HTTP status code&#10;// variables - Collection variables object&#10;// context - Runtime context object&#10;&#10;// Available functions:&#10;// setContext(key, value) - Store data for next request&#10;// getContext(key) - Retrieve stored data&#10;// getVariable(key) - Get collection variable&#10;// getResponseValue('path.to.value') - Get value from JSON response&#10;// getResponseHeader('header-name') - Get response header value&#10;&#10;// Example 1: Extract and store auth token&#10;// const token = getResponseValue('data.token');&#10;// setContext('authToken', token);&#10;&#10;// Example 2: Store user ID for next request&#10;// const userId = response.user.id;&#10;// setContext('userId', userId);&#10;&#10;// Example 3: Validate response with variable&#10;// const expectedStatus = getVariable('expectedStatusCode');&#10;// if (statusCode !== parseInt(expectedStatus)) {&#10;//   console.error('Unexpected status:', statusCode);&#10;// }&#10;&#10;// Example 4: Extract from nested response&#10;// const accessToken = getResponseValue('auth.tokens.access');&#10;// const refreshToken = getResponseValue('auth.tokens.refresh');&#10;// setContext('accessToken', accessToken);&#10;// setContext('refreshToken', refreshToken);"
          value={tabData.postRequestScript}
          onChange={(e) => onUpdateTabData({ postRequestScript: e.target.value })}
          sx={{ fontFamily: 'monospace' }}
        />
      </div>
      {tabData.response && (
        <div className="ResponseViewer">
          <h3>Response:</h3>
          {tabData.statusCode && (
            <div className="StatusCodeSection">
              <h4>Status Code</h4>
              <div className="StatusCodeValue">
                <span className={`StatusCode status-${Math.floor(tabData.statusCode / 100)}xx`}>
                  {tabData.statusCode}
                </span>
                <span className="StatusCodeText">
                  {getStatusText(tabData.statusCode)}
                </span>
              </div>
            </div>
          )}
          {tabData.responseHeaders && (
            <div className="ResponseHeadersSection">
              <h4>Headers</h4>
              <div className="ResponseHeadersList">
                {Object.entries(tabData.responseHeaders).map(([key, value]) => (
                  <div key={key} className="ResponseHeaderItem">
                    <span className="ResponseHeaderKey">{key}:</span>
                    <span className="ResponseHeaderValue">{value}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
          <pre>
            <code className={`language-${tabData.responseType}`}>
              {tabData.response}
            </code>
          </pre>
        </div>
      )}
    </div>
  );
};

export default RequestPanel;
