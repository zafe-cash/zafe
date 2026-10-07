// Build-time links (build.sh validates them and passes them in the environment).
const repo = 'https://github.com/zafe-cash/zafe';

// The GitHub Releases page (set by site.yml); empty falls back to the source.
export const downloadUrl = process.env.ZAFE_DOWNLOAD_URL || '';
export const sourceUrl = process.env.ZAFE_SOURCE_URL || repo;
// The call to action: the app once it's downloadable, the source until then.
export const getUrl = downloadUrl || sourceUrl;
export const getLabel = downloadUrl ? 'Get Zafe' : 'View code';
export const getNote = downloadUrl
  ? 'Two phones and a few testnet coins.'
  : 'Testnet builds soon. The code is open now.';
export const specUrl = `${sourceUrl}/blob/main/spec.md`;

// Shared by every page's social tags (Base.astro). The image is 1200x630, same-origin.
export const siteName = 'Zafe';
export const socialImage = {
  path: '/assets/og.png',
  width: 1200,
  height: 630,
  alt: 'Zafe: the multisig nobody can see',
};

// Same policy as public/_headers, for hosts that can't set headers. frame-ancestors only
// works as a header.
export const csp =
  "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; " +
  "font-src 'self'; connect-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'none'";
