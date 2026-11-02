RSpec.describe 'suite' do
  it 'value (a)+' do
    skip 'exact target intentionally skipped' if ENV['VER01_SKIP_EXACT']
    expect(1).to eq(1)
  end

  it('value (a)+ suffix') { raise 'suffix sibling ran' }
  it('prefix value (a)+') { raise 'prefix sibling ran' }
end
