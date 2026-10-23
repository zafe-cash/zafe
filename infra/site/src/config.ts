// Build-time links (build.sh validates them and passes them in the environment).
const repo = 'https://github.com/zafe-cash/zafe';

// The mainnet beta release to download (set by site.yml: the exact release page, since
// pre-releases never show under /releases/latest); empty falls back to the source.
export const downloadUrl = process.env.ZAFE_DOWNLOAD_URL || '';
// The testnet build, for trying Zafe with coins that have no value (set by site.yml).
export const testnetUrl = process.env.ZAFE_TESTNET_URL || '';
// Launch news goes out on X: no signup form, no address collected.
export const xUrl = 'https://x.com/zafecash';
export const sourceUrl = process.env.ZAFE_SOURCE_URL || repo;
// Google Play listing (live since 2026-10-16).
export const playUrl = 'https://play.google.com/store/apps/details?id=xyz.zafe.zafe';
// The APK itself, from the release page's tag (mainnet-v0.1.22 → zafe-0.1.22-mainnet-arm64.apk).
const tag = downloadUrl.match(/\/releases\/tag\/(mainnet-v([0-9][0-9A-Za-z.+-]*))$/);
export const apkVersion = tag ? tag[2] : '';
export const apkUrl = tag ? `${repo}/releases/download/${tag[1]}/zafe-${tag[2]}-mainnet-arm64.apk` : '';
// The release signing certificate: the same for every release (the release notes say so).
export const apkCertSha256 = 'd8a2f2c12d99bada3f259bf3a9e8c457318d24bde0b15b1c3abae467f349f69c';
// The call to action: Google Play, with the other ways to install on /download.
export const getUrl = '/download';
export const getLabel = 'Get Zafe';
export const specUrl = `${sourceUrl}/blob/main/spec.md`;

// Shared by every page's social tags (Base.astro). The image is 1200x630, same-origin.
export const siteName = 'Zafe';
export const socialImage = {
  path: '/assets/og-mainnet.png',
  width: 1200,
  height: 630,
  alt: 'Zafe: the multisig nobody can see',
};

// Same policy as public/_headers, for hosts that can't set headers. frame-ancestors only
// works as a header.
export const csp =
  "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; media-src 'self'; " +
  "font-src 'self'; connect-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'self'";
