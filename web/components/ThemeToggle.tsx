import React from 'react';

export default function ThemeToggle() {
  function toggle() {
    const root = document.documentElement;
    const current = root.getAttribute('data-theme') === 'light' ? 'light' : 'dark';
    const next = current === 'light' ? 'dark' : 'light';
    root.setAttribute('data-theme', next);
    try { localStorage.setItem('zap_theme', next); } catch {}
  }

  return (
    <button className="button theme-toggle" onClick={toggle} aria-label="Toggle theme">
      <span role="img" aria-hidden>🌓</span> Theme
    </button>
  );
};