import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './app/App'
import './styles.css'
import 'flag-icons/css/flag-icons.min.css'

// React is mounted once into the Tauri webview root. All desktop-specific
// behavior is reached through App -> desktop/transport.ts instead of globals.
ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
)
