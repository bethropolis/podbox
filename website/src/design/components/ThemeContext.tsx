import React, { createContext, useContext, useEffect, useState } from 'react';

type Theme = 'mocha' | 'latte';

interface ThemeContextType {
  theme: Theme;
  toggleTheme: () => void;
  isDark: boolean;
}

const ThemeContext = createContext<ThemeContextType>({
  theme: 'mocha',
  toggleTheme: () => {},
  isDark: true,
});

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  // SSR-safe: server renders mocha; client reconciles from localStorage.
  // A head inline script sets the class pre-paint to avoid a flash.
  const [theme, setTheme] = useState<Theme>(() => {
    if (typeof localStorage === 'undefined') return 'mocha';
    const saved = localStorage.getItem('podbox-theme');
    if (saved === 'latte') return 'latte';
    return 'mocha';
  });

  useEffect(() => {
    const root = document.documentElement;
    if (theme === 'latte') {
      root.classList.remove('dark');
      root.classList.add('light');
    } else {
      root.classList.remove('light');
      root.classList.add('dark');
    }
    localStorage.setItem('podbox-theme', theme);
  }, [theme]);

  const toggleTheme = () => {
    setTheme(prev => (prev === 'mocha' ? 'latte' : 'mocha'));
  };

  return (
    <ThemeContext.Provider value={{ theme, toggleTheme, isDark: theme === 'mocha' }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  return useContext(ThemeContext);
}
