/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: 'http://127.0.0.1:4000/api/:path*',
      },
      {
        source: '/ws',
        destination: 'http://127.0.0.1:4000/ws',
      },
    ];
  },
};

module.exports = nextConfig;
