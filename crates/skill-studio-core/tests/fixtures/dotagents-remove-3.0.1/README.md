# Recorded Dotagents Remove

Recorded with `npx -y @sentry/dotagents@3.0.1 remove alpha` under an isolated
HOME, npm cache and configuration. The command exited 0. The skill's local
folder was removed. Remove did not fetch the source repository.

The before files use valid version 1 schemas, comments and single quotes.
The after manifest retains the declaration comment. The after lock includes
an empty `[skills]` container. Global Remove leaves `.gitignore` unchanged.
Tests replay these exact bytes without a process or network dependency.

3.0.1 identifies the recording. It is not a production dependency pin.
