import React from 'react';
import { ThemeProvider } from '../components/ThemeContext';
import { Navbar } from '../components/Navbar';

interface TopbarProps {
  currentPath: string;
  isDocsPage?: boolean;
  version: string;
}

// Site chrome island: navbar only. The search palette is a separate
// `client:idle` island (owns its state, lazy-loads its index on open),
// so zero palette JS ships on initial load.
export function Topbar({ currentPath, isDocsPage = false, version }: TopbarProps) {
  return (
    <ThemeProvider>
      <Navbar currentPath={currentPath} isDocsPage={isDocsPage} version={version} />
    </ThemeProvider>
  );
}
