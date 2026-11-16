const repo = 'https://github.com/zafe-cash/zafe';

export const downloadUrl = process.env.ZAFE_DOWNLOAD_URL || '';
export const testnetUrl = process.env.ZAFE_TESTNET_URL || '';
export const xUrl = 'https://x.com/zafecash';
export const sourceUrl = process.env.ZAFE_SOURCE_URL || repo;
export const playUrl = 'https://play.google.com/store/apps/details?id=xyz.zafe.zafe';
const tag = downloadUrl.match(/\/releases\/tag\/(mainnet-v([0-9][0-9A-Za-z.+-]*))$/);
export const apkVersion = tag ? tag[2] : '';
export const apkUrl = tag ? `${repo}/releases/download/${tag[1]}/zafe-${tag[2]}-mainnet-arm64.apk` : '';
export const apkCertSha256 = 'd8a2f2c12d99bada3f259bf3a9e8c457318d24bde0b15b1c3abae467f349f69c';
export const getUrl = '/download';
export const getLabel = 'Get Zafe';
export const specUrl = `${sourceUrl}/blob/main/spec.md`;

export const siteName = 'Zafe';
export const socialImage = {
  path: '/assets/og-mainnet.png',
  width: 1200,
  height: 630,
  alt: 'Zafe: the multisig nobody can see',
};

export const csp =
  "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; media-src 'self'; " +
  "font-src 'self'; connect-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'self'";
