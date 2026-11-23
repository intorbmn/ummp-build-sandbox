# Reporting a security vulnerability

Please do not report exploitable vulnerabilities in public issues or pull requests.

1. Open the repository's **Security** tab and use **Report a vulnerability** to create a
   private GitHub Security Advisory.
2. Include the affected commit or release, affected component, reproduction steps, and
   impact. Redact tokens, API keys, personal data, and private project files.
3. Allow maintainers reasonable time to investigate and prepare a fix before public
   disclosure. We will coordinate a release and advisory with the reporter.

If private vulnerability reporting is unavailable, contact a repository maintainer through
their GitHub profile and ask for a private disclosure channel. Do not attach exploit code
to a public issue.

## Scope

UMMP accepts untrusted mod archives, source trees, descriptors, build output, plugin
responses, REST/MCP requests, and AI-generated proposals. Security reports are welcome
for path traversal, archive/resource exhaustion, command execution, secret disclosure,
authorization bypass, unsafe deserialization, and integrity failures.

## Supported versions

Security fixes are prioritized for the latest published release and the current default
branch. Older releases may require upgrading before a fix is available.
