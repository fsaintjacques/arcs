import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { initEngine } from './session';
import './styles.css';

// The engine is Rust compiled to wasm: instantiate it before the first render,
// so every hook can build sessions synchronously from then on.
initEngine().then(() => {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
});
