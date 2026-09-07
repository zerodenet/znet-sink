# Unified bypass policy

The Network settings page owns one `bypass` policy: `localNetworks` and `rules`.
The TUN page links to that editor. Matching destinations use the host's network
directly in both rule and global modes. This does not promise that every direct
connection avoids the core process: native proxy APIs cannot express every CIDR.

On load, configurations without `bypass` merge `localProxy.bypass` and
`tun.excludeCidrs`. The original complete recommended proxy list becomes the
local-network preset; explicitly empty or customized legacy lists are preserved
without automatically enabling that preset. Normalization deduplicates and
canonicalizes IP networks. Once migrated, `bypass` is authoritative; the two
legacy fields are generated projections, so deleting a rule cannot resurrect it
from the previous TUN projection. Kernel-settings v2 export/import carries `bypass`; v1 imports migrate. Older clients reject v2 rather than silently discarding domain exceptions.

The local-network preset covers loopback, RFC1918, IPv4 link-local, IPv6
link-local and ULA addresses, plus localhost, local domains and simple names.
It preserves native routing, including an existing VPN's more-specific routes;
it does not force those destinations through the default physical gateway.

Native proxy exceptions receive only patterns that can be represented exactly.
Arbitrary CIDRs are enforced by the core instead of being widened. Core
`route.bypass` conditions run before mode selection. IP exceptions also become
TUN route exclusions; profile-owned exclusions remain additive. Domain exceptions
feed core routing and, when managed DNS is enabled, use native DNS dispatch and
retain reverse mapping/Fake-IP ownership. Domain matching needs an observable
hostname (proxy request, managed DNS mapping or supported sniffing); an encrypted
DNS/ECH connection exposing only an IP must be handled by an IP/CIDR rule.

Updates reuse the existing configuration rollback path. Network changes rebuild
the app-owned TUN through the managed restart/restore path; domain-only changes
recompose the runtime. A running core must advertise `route_bypass_v1` before a
changed policy is published or a restart begins. Ship the updated core with this
client: an older core does not understand the new route contract. No release
version was bumped as part of this source change.

On macOS, bypass mutations join the existing authorized `networksetup` command
transaction. The guard stores original lists per network service before writes,
including empty lists and newly discovered services. An old marker is upgraded
before applying bypass; repeated enables verify native lists rather than trusting
the saved settings alone. Disable/crash recovery restores the captured lists.
The privilege helper architecture is unchanged.

Regression coverage includes legacy migration, removal, exact CIDR projection,
portable settings, native argument generation, global/rule routing and snapshot
reload isolation. Browser cases use fixtures. These tests are intended for CI or
an isolated environment; no live network mutation or local runtime test was used
while implementing this change.
