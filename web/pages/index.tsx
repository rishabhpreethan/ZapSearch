import React, { useEffect } from 'react';
import { useRouter } from 'next/router';

export default function Home() {
  const router = useRouter();
  useEffect(() => {
    router.replace('/search');
  }, [router]);
  return (
    <div style={{ padding: '2rem', fontFamily: 'sans-serif' }}>
      <h1>Redirecting to Search…</h1>
      <p>If you are not redirected automatically, <a href="/search">click here</a>.</p>
    </div>
  );
}
