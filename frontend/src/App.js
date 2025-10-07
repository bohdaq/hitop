import './App.css';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import styles from './App.css'

function App() {
  return (
    <div className="App">
      <div className='UrlContainer'>
        <TextField fullWidth id="outlined-basic" label="URL" variant="outlined" />
      </div>
      <Button size='large' variant="contained">Make Request</Button>
    </div>
  );
}

export default App;
