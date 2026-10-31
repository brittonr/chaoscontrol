# Security policy

## Support boundary

ChaosControl has no production-supported release at present. Passing a bounded
simulation, replay, dependency audit or lifecycle gate does not establish a
security boundary for arbitrary guests, inputs, hosts or workloads.

The supported product scope is Rust workloads on a local x86_64 Linux/KVM host.
See [product scope](docs/product-scope-status.md) and
[release preparation](docs/release-preparation.md). There is no hosted service,
multi-tenant isolation claim or general-purpose hostile-guest sandbox claim.

## Operating precautions

- Run reviewed workloads on a dedicated test host without production credentials,
  sensitive host data or production network access. KVM access is privileged;
  guest execution and fault campaigns are not safe substitutes for host isolation.
- Keep the dashboard on loopback. Its CLI defaults to `127.0.0.1` and explicitly
  provides no authentication. Do not expose it to untrusted networks.
- Treat kernels, initrds, disk images, recordings, snapshots and imported evidence
  as untrusted inputs. Bounded admission and BLAKE3 identity checks do not confer
  trust, authorization or confidentiality.
- Snapshots, guest memory, disk images and logs can contain workload secrets.
  Review and redact artifacts before sharing them. Preserve exact originals
  privately when redaction would invalidate replay identities.
- Run the existing dependency audit and policy gates before accepting a candidate;
  old passing audit output is not a current advisory assessment.

## Reporting a vulnerability

Do not put exploitable details, credentials, private guest data or memory dumps
in a public issue. If GitHub offers **Report a vulnerability** on this repository,
use its private advisory form. Otherwise request a private reporting channel from
the repository owner before sending technical details. No response-time or
backport commitment is currently offered.

Include the exact source revision, host/kernel/KVM versions, affected command,
minimal reviewed reproducer, observed impact and artifact identities. Keep
sensitive artifacts private. Public release requires maintainers to verify a
working private reporting channel; this document does not assert that GitHub
private reporting is enabled.
