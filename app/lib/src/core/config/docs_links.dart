/// Every page of the website's docs (https://zafe.cash/docs) that the app links to, in
/// one place. scripts/check_docs_links.py checks that each path exists in the built site
/// (infra/site/dist) or is a known redirect, so a renamed docs page can't break a link
/// in a shipped build. Older builds link to `https://zafe.cash/relay`, which redirects
/// to [kDocsRunAServerUrl] for good.
library;

/// The site's docs root.
const kDocsBaseUrl = 'https://zafe.cash/docs';

/// Running your own server (Compose, Caddy, backups, the create token): the server
/// list's "Run your own server →".
const kDocsRunAServerUrl = '$kDocsBaseUrl/run-a-server';
