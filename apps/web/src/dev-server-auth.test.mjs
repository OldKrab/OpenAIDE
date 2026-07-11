import assert from "node:assert/strict";
import test from "node:test";
import {
  authConfigFromEnv,
  isAllowedBrowserOrigin,
  isAuthorized,
  writeUnauthorized,
} from "./dev-server-auth.mjs";

test("accepts only the exact configured browser origin tuple", () => {
  const requestHeaders = {
    host: "localhost:5574",
    "x-forwarded-proto": "https",
  };

  assert.equal(isAllowedBrowserOrigin("https://localhost:5574", requestHeaders), true);
  assert.equal(isAllowedBrowserOrigin("http://localhost:5574", requestHeaders), false);
  assert.equal(isAllowedBrowserOrigin("https://localhost:4173", requestHeaders), false);
});

test("disables auth when no demo credential is configured", () => {
  const config = authConfigFromEnv({});

  assert.equal(config.enabled, false);
  assert.equal(isAuthorized({}, config), true);
});

test("accepts only matching basic auth credentials", () => {
  const config = authConfigFromEnv({
    OPENAIDE_WEB_USERNAME: "friend",
    OPENAIDE_WEB_PASSWORD: "expected-value",
  });
  const valid = `Basic ${Buffer.from("friend:expected-value").toString("base64")}`;
  const badPassword = `Basic ${Buffer.from("friend:wrong-value").toString("base64")}`;
  const badUser = `Basic ${Buffer.from("other:expected-value").toString("base64")}`;

  assert.equal(isAuthorized({ authorization: valid }, config), true);
  assert.equal(isAuthorized({ authorization: badPassword }, config), false);
  assert.equal(isAuthorized({ authorization: badUser }, config), false);
  assert.equal(isAuthorized({}, config), false);
});

test("writes a basic auth challenge without exposing the password", () => {
  const config = authConfigFromEnv({
    OPENAIDE_WEB_AUTH_REALM: 'Demo "quoted"',
    OPENAIDE_WEB_PASSWORD: "expected-value",
  });
  const response = fakeResponse();

  writeUnauthorized(response, config);

  assert.equal(response.status, 401);
  assert.equal(response.body, "Authentication required");
  assert.equal(
    response.headers["www-authenticate"],
    'Basic realm="Demo \\"quoted\\"", charset="UTF-8"',
  );
  assert.doesNotMatch(JSON.stringify(response.headers), /expected-value/);
});

function fakeResponse() {
  return {
    status: undefined,
    headers: undefined,
    body: "",
    writeHead(status, headers) {
      this.status = status;
      this.headers = headers;
    },
    end(body) {
      this.body = body;
    },
  };
}
