import fs from 'node:fs';
import assert from 'node:assert/strict';

const parser = fs.readFileSync('src-tauri/src/services/subscription.rs', 'utf8');
const wrapper = fs.readFileSync('src-tauri/src/services/subscription_wrapper.rs', 'utf8');
const editor = fs.readFileSync('src/lib/components/tabs/SubscriptionsTab.svelte', 'utf8');

assert.match(parser, /plaintext Zero JSON subscriptions are not supported/);
assert.match(parser, /"zero" \| "zero-base64-json" \| "base64-json" \| "znet-sink"/);
assert.doesNotMatch(parser, /return Ok\(ParsedSubscriptionConfig \{\s*content: value,\s*format: "zero-json"/s);

assert.match(wrapper, /"auto" =>/);
assert.match(wrapper, /"zero" => original::parse_subscription_content\(content, "zero-base64-json"\)/);
assert.match(wrapper, /CLIENT_USER_AGENT/);
assert.match(wrapper, /CLIENT_USER_AGENT_PREFIX/);
assert.match(wrapper, /contains_client_identity/);
assert.doesNotMatch(wrapper, /format\s*=\s*Some\("zero-json"/);

assert.match(editor, /\{ value: 'auto', label: '自动检测' \}/);
assert.match(editor, /\{ value: 'zero', label: 'Zero' \}/);
assert.match(editor, /\{ value: 'clash', label: 'Clash' \}/);
assert.doesNotMatch(editor, /value: 'zero-json'/);
assert.doesNotMatch(editor, /DraggableModal/);
assert.match(editor, /\$lib\/components\/ui\/dialog/);
assert.match(editor, /末尾自动追加 ZNet-Sink/);

console.log('subscription contract checks passed');
