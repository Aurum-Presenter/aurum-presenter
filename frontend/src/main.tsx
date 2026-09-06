import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { countVisit } from './pwa/install';
import { acceptLaunchFiles } from './pwa/SharePage';
import './index.css';

// The install banner appears on the third visit, and files opened with Aurum from the OS are
// picked up before the first render so they are not lost to a navigation.
countVisit();
acceptLaunchFiles();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
