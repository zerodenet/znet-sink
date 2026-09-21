import assert from 'node:assert/strict';
import {
  canApprove,
  initialPermissionSelection,
  permissionKey,
  requiredPermissionSelection,
  supportedPermissionSelection,
} from '../src/lib/services/plugin-permissions.ts';

const request = (capability) => ({capability, scope: 'self'});
const component = {
  blocked: null,
  review: {key: 'plugin/component'},
  configuration: null,
  permissions: [
    {request: request('required.one'), required: true, supported: true, granted: false},
    {request: request('optional.granted'), required: false, supported: true, granted: true},
    {request: request('optional.available'), required: false, supported: true, granted: false},
    {request: request('optional.unavailable'), required: false, supported: false, granted: false},
  ],
};

assert.deepEqual(initialPermissionSelection(component), [
  permissionKey(request('required.one')),
  permissionKey(request('optional.granted')),
]);
assert.deepEqual(requiredPermissionSelection(component), [permissionKey(request('required.one'))]);
assert.deepEqual(supportedPermissionSelection(component), [
  permissionKey(request('required.one')),
  permissionKey(request('optional.granted')),
  permissionKey(request('optional.available')),
]);
assert.equal(canApprove(component, initialPermissionSelection(component)), true);
assert.equal(canApprove(component, []), false);

console.log('Plugin permission selection contract passed.');
