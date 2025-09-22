# Web Frontend

A minimal Next.js app that queries the Rust backend.

Environment variables:
- `NEXT_PUBLIC_BACKEND_URL` — URL of the Rust server (e.g., `https://your-backend.example.com`)

Scripts:
- `pnpm dev` or `npm run dev` — run locally at http://localhost:3000
- `pnpm build` — production build

Deploy to Vercel:
1. Push this repo to GitHub.
2. Import the repo in Vercel.
3. Set Environment Variable `NEXT_PUBLIC_BACKEND_URL` to your backend URL.
4. Deploy.
