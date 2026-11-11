/// <reference types="vite/client" />

declare const __SITE_RELEASE__: string;
declare const __SITE_BRANCH__: string;

interface ImportMetaEnv {
  readonly VITE_SENTRY_DSN?: string;
}
