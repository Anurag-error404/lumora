# Security Policy

Thank you for helping keep LUMORA secure.

We take security seriously, especially because LUMORA is designed to manage users' personal photo libraries locally. If you discover a security vulnerability, please report it responsibly so we can investigate and fix it before public disclosure.

## Supported Versions

The latest stable release is actively supported with security updates.

| Version | Supported |
| -------- | --------- |
| Latest Release (master) | ✅ |
| Development (dev) | ✅ |
| Older Releases | ❌ |

## Reporting a Vulnerability

Please **do not create a public GitHub issue** for security vulnerabilities.

Instead, report vulnerabilities privately by emailing:

**security@yourdomain.com**

*(Replace this with your actual security contact.)*

If email is not yet available, you may open a **private GitHub Security Advisory** if the repository has GitHub Security Advisories enabled.

When reporting a vulnerability, please include:

- A clear description of the issue
- Steps to reproduce
- Potential impact
- Screenshots or proof-of-concept (if applicable)
- Environment information (OS, LUMORA version, etc.)

## What to Expect

After receiving your report, we aim to:

- Acknowledge your report within **72 hours**
- Investigate and validate the issue
- Keep you informed about the progress
- Release a fix as soon as reasonably possible
- Publicly credit you (if you wish) once the issue has been resolved

## Responsible Disclosure

Please do not publicly disclose vulnerabilities until a fix has been released.

We appreciate responsible disclosure and will work with researchers to resolve issues quickly.

## Scope

Examples of issues that should be reported include:

- Remote code execution
- Arbitrary file access
- Path traversal
- Local privilege escalation
- Authentication bypass (if applicable)
- Encryption weaknesses
- Sensitive information disclosure
- Dependency vulnerabilities with practical impact

## Out of Scope

The following are generally considered out of scope:

- Feature requests
- UI/UX issues
- Crashes without security impact
- Denial-of-service requiring physical access to the user's machine
- Vulnerabilities in unsupported third-party software
- Issues requiring deliberate modification of the application's source code

## Privacy

LUMORA is built with a **local-first** philosophy.

By default:

- Photos remain on the user's device.
- No media is uploaded to our servers.
- No telemetry is collected without explicit user consent.
- AI processing is intended to run locally.

If a security issue could compromise these guarantees, please report it immediately.

Thank you for helping make LUMORA safer for everyone.
