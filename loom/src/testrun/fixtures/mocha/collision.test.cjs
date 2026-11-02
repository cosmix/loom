const assert = require('node:assert/strict');

describe('suite', () => {
  const exact = process.env.VER01_SKIP_EXACT ? it.skip : it;
  exact('value (a)+', () => assert.equal(1, 1));
  it('value (a)+ suffix', () => assert.fail('suffix sibling ran'));
  it('prefix value (a)+', () => assert.fail('prefix sibling ran'));
});
