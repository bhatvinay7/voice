const path = require('path');
const BACKEND_URL = process.env.BACKEND_URL || 'http://127.0.0.1:4000';

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  outputFileTracingRoot: path.join(__dirname, '../../'),
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: `${BACKEND_URL}/api/:path*`,
      },
      {
        source: '/ws',
        destination: `${BACKEND_URL}/ws`,
      },
    ];
  },
};

module.exports = nextConfig;
