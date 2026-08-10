/**
 * Ambient bindings for the httpyac script VM.
 *
 * The model declarations below are imported from the adjacent upstream
 * httpyac/src/models snapshot. Replace that snapshot manually when updating
 * httpyac; this file should only change if the global bindings change.
 */
import type { HttpFile } from './src/models/httpFile';
import type { HttpRegion } from './src/models/httpRegion';
import type { HttpRequest } from './src/models/httpRequest';
import type { HttpResponse } from './src/models/httpResponse';
import type { OpenIdInformation } from './src/models/openIdInformation';
import type { RequestClient } from './src/models/requestClient';
import type { TestFunction } from './src/models/testFunction';
import type { Variables } from './src/models/variables';
import type { sleep as httpyacSleep } from './src/utils/promiseUtils';

declare global {
  const $global: Variables;
  const $requestClient: RequestClient;
  const httpFile: HttpFile;
  const httpRegion: HttpRegion;
  const oauth2Session: OpenIdInformation | undefined;
  const request: HttpRequest;
  const response: HttpResponse;
  const sleep: typeof httpyacSleep;
  const test: TestFunction;
  const __dirname: string;
  const __filename: string;
}

export {};
