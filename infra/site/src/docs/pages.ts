// The docs: every page, in sidebar order, grouped in sections. The sidebar, the /docs
// index, breadcrumbs, "Related" lists and sitemap.xml all read this list, so a page is
// added here and as src/pages/docs/<slug>.astro. URLs are flat (/docs/<slug>): the
// section shows only in the sidebar and the breadcrumb, so a page can change section
// without its link breaking. The app links to some of these pages
// (app/lib/src/core/config/docs_links.dart; scripts/check_docs_links.py checks them).

/** The app version and date the steps were last checked against (app/lib). */
export const checked = { version: '0.1.23', date: '2026-10-27' };

export interface DocPage {
  slug: string;
  /** The H1: a task, in the reader's words. */
  title: string;
  /** Shorter label for the sidebar, when the title is long. */
  label?: string;
  /** One line under the H1; also the meta description and the index's line. */
  summary: string;
  /** Two or three slugs for "Related". */
  related: string[];
}

export interface DocSection {
  id: string;
  title: string;
  pages: DocPage[];
}

export const sections: DocSection[] = [
  {
    id: 'start',
    title: 'Start',
    pages: [
      {
        slug: 'what-is-zafe',
        title: 'What Zafe is',
        label: 'What is Zafe',
        summary: 'A shielded Zcash wallet that a group controls together from their phones.',
        related: ['create-a-vault', 'security', 'servers'],
      },
      {
        slug: 'create-a-vault',
        title: 'Create a vault',
        summary: 'Pick the members and the approval rule, then invite your co-signers.',
        related: ['join-a-vault', 'safety-words', 'backups'],
      },
      {
        slug: 'join-a-vault',
        title: 'Join a vault',
        summary: 'Open the invite a co-signer sent you, then compare safety words.',
        related: ['safety-words', 'backups', 'create-a-vault'],
      },
      {
        slug: 'safety-words',
        title: 'Compare safety words',
        label: 'Safety words',
        summary: 'Four words prove that every phone in the setup talks to the same people.',
        related: ['create-a-vault', 'join-a-vault', 'lost-phone'],
      },
      {
        slug: 'backups',
        title: 'Back up your key share',
        label: 'Backups',
        summary: 'An encrypted file and 12 words let you restore your seat on a new phone.',
        related: ['lost-phone', 'security', 'create-a-vault'],
      },
    ],
  },
  {
    id: 'payments',
    title: 'Payments',
    pages: [
      {
        slug: 'receive',
        title: 'Receive ZEC',
        label: 'Receive',
        summary: 'Share the vault’s address. Every member sees what arrives.',
        related: ['send', 'what-is-zafe'],
      },
      {
        slug: 'send',
        title: 'Send a payment',
        label: 'Send',
        summary: 'Propose it, enough members approve it, and it goes out on its own.',
        related: ['approve', 'waiting-and-expired', 'receive'],
      },
      {
        slug: 'approve',
        title: 'Approve a payment',
        label: 'Approve',
        summary: 'Your phone rebuilds the transaction and checks it before you sign.',
        related: ['send', 'waiting-and-expired', 'security'],
      },
      {
        slug: 'waiting-and-expired',
        title: 'Finish a waiting or expired payment',
        label: 'Waiting and expired',
        summary: 'Why a payment waits, how long it can, and what to do when it expires.',
        related: ['approve', 'send', 'troubleshooting'],
      },
    ],
  },
  {
    id: 'phones',
    title: 'Phones',
    pages: [
      {
        slug: 'lost-phone',
        title: 'Replace a lost phone',
        label: 'Lost phone',
        summary: 'Restore your backup, or let your co-signers move your seat to a new phone.',
        related: ['backups', 'safety-words', 'security'],
      },
    ],
  },
  {
    id: 'servers',
    title: 'Servers',
    pages: [
      {
        slug: 'servers',
        title: 'Choose a server',
        summary: 'What the vault’s server does, what it sees, and whose to use.',
        related: ['move-servers', 'run-a-server', 'tor'],
      },
      {
        slug: 'move-servers',
        title: 'Move to another server',
        summary: 'Take the vault to a new server. Funds stay where they are.',
        related: ['servers', 'run-a-server', 'troubleshooting'],
      },
    ],
  },
  {
    id: 'run-a-server',
    title: 'Run a server',
    pages: [
      {
        slug: 'run-a-server',
        title: 'Run your own server',
        summary: 'One Docker container and one SQLite file, behind HTTPS on your domain.',
        related: ['servers', 'move-servers', 'security'],
      },
    ],
  },
  {
    id: 'privacy-security',
    title: 'Privacy & security',
    pages: [
      {
        slug: 'tor',
        title: 'Use Tor',
        summary: 'Hide this phone’s IP address from the servers Zafe talks to.',
        related: ['servers', 'security', 'troubleshooting'],
      },
      {
        slug: 'security',
        title: 'Know what keeps funds safe',
        label: 'Security',
        summary: 'Who can do what, what is checked where, and what isn’t done yet.',
        related: ['approve', 'backups', 'servers'],
      },
    ],
  },
  {
    id: 'help',
    title: 'Help',
    pages: [
      {
        slug: 'updates',
        title: 'Update Zafe',
        label: 'Updates',
        summary: 'Keep every phone in the vault on a current version, before NU7 on November 5.',
        related: ['troubleshooting', 'move-servers'],
      },
      {
        slug: 'troubleshooting',
        title: 'Fix common problems',
        label: 'Troubleshooting',
        summary: 'What the app’s messages mean and what to do about each.',
        related: ['waiting-and-expired', 'move-servers', 'updates'],
      },
    ],
  },
];

/** The /docs index's short list. */
export const popular = ['create-a-vault', 'send', 'lost-phone', 'run-a-server'];

export const allPages: DocPage[] = sections.flatMap((s) => s.pages);

export function docHref(slug: string): string {
  return `/docs/${slug}`;
}

export function findPage(slug: string): { page: DocPage; section: DocSection } {
  for (const section of sections) {
    const page = section.pages.find((p) => p.slug === slug);
    if (page) return { page, section };
  }
  throw new Error(`no docs page "${slug}" in src/docs/pages.ts`);
}
