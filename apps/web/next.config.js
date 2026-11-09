const path = require('path');
const API_URL = process.env.API_URL || process.env.BACKEND_URL || 'http://127.0.0.1:4000';
const VOICE_SERVER_URL = process.env.VOICE_SERVER_URL || process.env.VOICE_BACKEND_URL || 'http://127.0.0.1:4001';

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  outputFileTracingRoot: path.join(__dirname, '../../'),
  async rewrites() {
    return [
      {
        source: '/api/calls/:path*',
        destination: `${VOICE_SERVER_URL}/api/calls/:path*`,
      },
      {
        source: '/ws',
        destination: `${VOICE_SERVER_URL}/ws`,
      },
      {
        source: '/api/telephony/:path*',
        destination: `${VOICE_SERVER_URL}/api/telephony/:path*`,
      },
      {
        source: '/api/:path*',
        destination: `${API_URL}/api/:path*`,
      },
    ];
  },
};

module.exports = nextConfig;
