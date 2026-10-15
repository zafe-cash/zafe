import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/backup/backup_prompt_screen.dart';

void main() {
  test('what skipping the backup risks, for the group', () {
    expect(skipBackupRisk(t: 2, n: 2), contains('every member is needed'));
    expect(
      skipBackupRisk(t: 2, n: 3),
      contains('no other member loses theirs'),
    );
    expect(
      skipBackupRisk(t: 3, n: 5),
      contains('no more than 1 other member loses theirs'),
    );
    expect(
      skipBackupRisk(t: 2, n: 5),
      contains('no more than 2 other members lose theirs'),
    );
  });
}
