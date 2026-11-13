# Security policy

## Reporting a vulnerability in shapekit

Report it privately through GitHub's security advisories: open the repository's **Security**
tab and choose **Report a vulnerability**
(<https://github.com/ErenTopaal/shapekit/security/advisories/new>). Do not open a public issue
or pull request for it. The report stays visible only to you and the maintainers until a fix is
published.

In scope:

- the tools in this repository (`shapekit`, `indexerlint`, the wallet-backend adapter, the
  testees), for example a corpus file or replay input that makes them execute code, write
  outside their output directory, or reach the network when they are documented as offline;
- the release process and its artifacts (`release.yml`, the corpus archive, `SHA256SUMS`, the
  provenance attestations);
- the CI workflows, for example a pull request that can obtain write access or secrets.

The supported version is the newest release. A fix lands on `main` and in the next release, and
the advisory is published with it.

## Vulnerabilities in other projects

The corpus exists to find bugs in SDKs and indexers. A finding that is a security issue in
another project (stellar-core, an SDK, wallet-backend or any other indexer) goes to that
project's own private disclosure channel, as its security policy describes, and not into a
public issue, a public finding or a corpus entry here. The finding is added to this repository
only after that project has published its fix or agreed to disclosure.
