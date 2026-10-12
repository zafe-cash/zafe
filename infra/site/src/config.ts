// Build-time links (build.sh validates them and passes them in the environment).
const repo = 'https://github.com/zafe-cash/zafe';

// The mainnet beta release to download (set by site.yml: the exact release page, since
// pre-releases never show under /releases/latest); empty falls back to the source.
export const downloadUrl = process.env.ZAFE_DOWNLOAD_URL || '';
// The testnet build, for trying Zafe with coins that have no value (set by site.yml).
export const testnetUrl = process.env.ZAFE_TESTNET_URL || '';
// Google Play waitlist: a plain mailto (the CSP allows no form posts, and an address is
// all we ask for). Cloudflare Email Routing forwards every @zafe.cash address.
export const waitlistUrl = 'mailto:waitlist@zafe.cash?subject=Zafe%20on%20Google%20Play';
export const sourceUrl = process.env.ZAFE_SOURCE_URL || repo;
// The call to action: the app once it's downloadable, the source until then.
export const getUrl = downloadUrl || sourceUrl;
export const getLabel = downloadUrl ? 'Get Zafe' : 'View code';
export const getNote = downloadUrl
  ? 'Android beta. Bring your co-signers.'
  : 'Builds soon. The code is open now.';
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
