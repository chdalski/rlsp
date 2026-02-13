# Security Principles

## Defense in Depth

Apply security at every layer. Do not rely on a single
control. Input validation at the boundary does not excuse
missing authorization checks deeper in.

## Input Boundaries

All external input is untrusted. This includes:

- User input (forms, query parameters, headers, cookies)
- API responses from third-party services
- File contents uploaded by users
- Environment variables set at deployment time
- Database records that may have been written by untrusted sources

Validate, sanitize, and constrain input at the system
boundary — where it enters your code. After validation,
internal code can trust the validated types.

## OWASP Top 10

Apply these defenses by default, not as an afterthought:

- **Injection** — use parameterized queries, avoid string
  concatenation for SQL, commands, or markup. Never
  interpolate user input into shell commands.
- **Broken authentication** — use established libraries for
  auth. Do not roll your own password hashing, session
  management, or token generation.
- **Sensitive data exposure** — do not log secrets, tokens,
  passwords, or PII. Do not hardcode credentials in source.
  Use environment variables or secret managers.
- **Broken access control** — check authorization on every
  request, not just at the UI layer. Default to deny.
- **Security misconfiguration** — use secure defaults.
  Disable debug modes, directory listings, verbose errors
  in production.
- **XSS** — encode output contextually (HTML, JS, URL).
  Use framework-provided escaping. Do not insert raw user
  content into templates.
- **Insecure deserialization** — do not deserialize
  untrusted data with formats that allow code execution
  (pickle, YAML load, Java serialization). Use safe
  alternatives.
- **Dependency vulnerabilities** — prefer well-maintained
  dependencies with active security response. Audit
  dependency trees for known CVEs when adding new packages.

## Secrets and Credentials

- Never commit secrets to version control.
- Never hardcode API keys, passwords, or tokens in source.
- Never log authentication tokens or credentials.
- Use `.env` files (gitignored) or secret managers for
  local development.
- Rotate secrets that may have been exposed.

## Error Handling

- Do not leak internal details (stack traces, SQL errors,
  file paths) in user-facing error messages.
- Log detailed errors server-side for debugging.
- Return generic error messages to users.

## Authorization

- Check permissions at the resource level, not just the
  route level.
- Default to deny — explicitly grant access, never
  implicitly allow.
- Validate that the authenticated user owns or has access
  to the requested resource.

## Cryptography

- Do not implement your own cryptographic algorithms.
- Use established libraries and current algorithms.
- Use constant-time comparison for secrets and tokens.
