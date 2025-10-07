import './App.css';
import { useState } from 'react';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import styles from './App.css'

function App() {
  const [url, setUrl] = useState('');
  const [loading, setLoading] = useState(false);

  const handleMakeRequest = async () => {
    if (!url) {
      alert('Please enter a URL');
      return;
    }

    setLoading(true);
    try {
      const response = await fetch(url);
      const data = await response.text();
      console.log('Response:', data);
      alert(`Request successful! Status: ${response.status}`);
    } catch (error) {
      console.error('Error:', error);
      alert(`Request failed: ${error.message}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="App">
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
      <Button 
        size='large' 
        variant="contained"
        onClick={handleMakeRequest}
        disabled={loading}
      >
        {loading ? 'Loading...' : 'Make Request'}
      </Button>
    </div>
  );
}

export default App;
