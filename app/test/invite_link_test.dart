import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/onboarding/invite_link.dart';

void main() {
  const invite = 'zafe-invite-v1:0a1b2c3d4e5f60718293a4b5c6d7e8f9';

  group('inviteLink', () {
    test('embeds a URL-safe invite as is', () {
      expect(inviteLink(invite), 'zafe://join?invite=$invite');
    });

    test('percent-encodes anything else and round-trips', () {
      const odd = 'zafe-invite-v2:a b&c=d/e+f';
      final link = inviteLink(odd);
      expect(link, isNot(contains(' ')));
      expect(link, isNot(contains('&c')));
      expect(inviteFromLink(Uri.parse(link)), odd);
    });

    test('trims the invite', () {
      expect(inviteLink('  $invite\n'), 'zafe://join?invite=$invite');
    });
  });

  group('inviteFromLink', () {
    String? from(String s) => inviteFromLink(Uri.parse(s));

    test('round-trips', () {
      expect(from(inviteLink(invite)), invite);
    });

    test('accepts an encoded colon and upper-case scheme/host', () {
      expect(
        from('zafe://join?invite=${invite.replaceFirst(':', '%3A')}'),
        invite,
      );
      expect(from('ZAFE://JOIN?invite=$invite'), invite);
      expect(from('zafe://join/?invite=$invite'), invite);
      // The testnet app's own scheme is read by every build.
      expect(from('zafe-testnet://join?invite=$invite'), invite);
    });

    test('rejects other schemes', () {
      expect(from('https://join?invite=$invite'), isNull);
      expect(from('http://zafe.xyz/join?invite=$invite'), isNull);
      expect(from('zcash://join?invite=$invite'), isNull);
      expect(from('zafe-invite-v1:abcd'), isNull);
    });

    test('rejects other hosts and paths', () {
      expect(from('zafe://pay?invite=$invite'), isNull);
      expect(from('zafe://join.evil.example?invite=$invite'), isNull);
      expect(from('zafe://join/other?invite=$invite'), isNull);
      expect(from('zafe:join?invite=$invite'), isNull);
    });

    test('rejects missing, empty, repeated or foreign invites', () {
      expect(from('zafe://join'), isNull);
      expect(from('zafe://join?invite='), isNull);
      expect(from('zafe://join?other=$invite'), isNull);
      expect(from('zafe://join?invite=$invite&invite=$invite'), isNull);
      expect(from('zafe://join?invite=hello'), isNull);
      expect(from('zafe://join?invite=zafe-backup-v1:abcd'), isNull);
    });

    test('rejects bad percent-encoding', () {
      expect(inviteFromLink(Uri.parse('zafe://join?invite=%zz')), isNull);
    });
  });

  group('extractInvite', () {
    test('accepts a raw invite or a link', () {
      expect(extractInvite(invite), invite);
      expect(extractInvite('  $invite \n'), invite);
      expect(extractInvite(inviteLink(invite)), invite);
    });

    test('finds the invite inside a shared message', () {
      expect(
        extractInvite('Join "Ops fund" on Zafe: ${inviteLink(invite)}'),
        invite,
      );
      expect(extractInvite('here:\n$invite\nthanks'), invite);
    });

    test('rejects everything else', () {
      expect(extractInvite(''), isNull);
      expect(extractInvite('   '), isNull);
      expect(extractInvite('hello world'), isNull);
      expect(extractInvite('https://example.com/join?invite=$invite'), isNull);
      expect(extractInvite('zafe://pay?invite=$invite'), isNull);
      expect(extractInvite('zafe-backup-v1:abcd'), isNull);
    });
  });

  group('https links (ZAFE_LINK_HOST set)', () {
    const host = 'zafe.example';
    String link(String i) => inviteLink(i, linkHost: host);
    String? from(String s) => inviteFromLink(Uri.parse(s), linkHost: host);

    test('puts the invite in the fragment on the join path', () {
      expect(link(invite), 'https://zafe.example/join#$invite');
      expect(link('  $invite\n'), 'https://zafe.example/join#$invite');
      expect(inviteLink(invite, linkHost: 'Zafe.Example'), link(invite));
    });

    test('round-trips, also when the invite needs encoding', () {
      expect(from(link(invite)), invite);
      const odd = 'zafe-invite-v2:a b#c?d/e+f%';
      final encoded = link(odd);
      expect(Uri.parse(encoded).hasQuery, isFalse);
      expect(from(encoded), odd);
    });

    test('accepts a trailing slash, upper-case host and an encoded colon', () {
      expect(from('https://zafe.example/join/#$invite'), invite);
      expect(from('HTTPS://ZAFE.EXAMPLE/join#$invite'), invite);
      expect(from('https://zafe.example:443/join#$invite'), invite);
      expect(
        from('https://zafe.example/join#${invite.replaceFirst(':', '%3A')}'),
        invite,
      );
    });

    test('rejects other hosts, schemes, ports and user info', () {
      expect(from('https://evil.example/join#$invite'), isNull);
      expect(from('https://zafe.example.evil.example/join#$invite'), isNull);
      expect(from('https://sub.zafe.example/join#$invite'), isNull);
      expect(from('http://zafe.example/join#$invite'), isNull);
      expect(from('https://zafe.example:8443/join#$invite'), isNull);
      expect(from('https://user@zafe.example/join#$invite'), isNull);
    });

    test('rejects other paths, a query, and missing or foreign fragments', () {
      expect(from('https://zafe.example/#$invite'), isNull);
      expect(from('https://zafe.example/join/x#$invite'), isNull);
      expect(from('https://zafe.example/joinx#$invite'), isNull);
      expect(from('https://zafe.example/join?invite=$invite'), isNull);
      expect(from('https://zafe.example/join?x=1#$invite'), isNull);
      expect(from('https://zafe.example/join'), isNull);
      expect(from('https://zafe.example/join#'), isNull);
      expect(from('https://zafe.example/join#hello'), isNull);
      expect(from('https://zafe.example/join#zafe-backup-v1:abcd'), isNull);
      expect(from('https://zafe.example/join#%zz'), isNull);
    });

    test('scheme links keep working', () {
      expect(from('zafe://join?invite=$invite'), invite);
    });

    test('isInviteLink recognises both forms, valid or not', () {
      bool isLink(String s) => isInviteLink(Uri.parse(s), linkHost: host);
      expect(isLink('https://zafe.example/join#hello'), isTrue);
      expect(isLink('zafe://join'), isTrue);
      expect(isLink('https://evil.example/join#$invite'), isFalse);
      expect(
        isInviteLink(Uri.parse('https://zafe.example/join#$invite')),
        isFalse,
      );
    });

    test('extractInvite finds an https link in a message', () {
      expect(
        extractInvite('Join on Zafe: ${link(invite)} thanks', linkHost: host),
        invite,
      );
      expect(
        extractInvite('https://evil.example/join#$invite', linkHost: host),
        isNull,
      );
    });
  });

  test('without a link host, https links are never invites', () {
    expect(
      inviteFromLink(Uri.parse('https://zafe.example/join#$invite')),
      isNull,
    );
    expect(extractInvite('https://zafe.example/join#$invite'), isNull);
  });
}
