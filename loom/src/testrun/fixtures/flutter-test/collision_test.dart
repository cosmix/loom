import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  group('suite', () {
    test('value (a)+', () => expect(true, isTrue),
        skip: Platform.environment.containsKey('VER01_SKIP_EXACT'));
    test('value (a)+ suffix', () => expect(true, isFalse));
    test('prefix value (a)+', () => expect(true, isFalse));
  });
}
