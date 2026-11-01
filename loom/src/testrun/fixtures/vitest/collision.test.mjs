describe('suite', () => {
  const exact = process.env.VER01_SKIP_EXACT ? it.skip : it;
  exact('value (a)+', () => expect(true).toBe(true));
  it('value (a)+ suffix', () => expect(true).toBe(false));
  it('prefix value (a)+', () => expect(true).toBe(false));
});
