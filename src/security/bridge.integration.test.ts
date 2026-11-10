// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only
// @vitest-environment node

import { spawn, type ChildProcess } from "node:child_process";
import {
  createCipheriv,
  createDecipheriv,
  createHash,
  hkdfSync,
} from "node:crypto";
import { once } from "node:events";
import { createInterface } from "node:readline";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import * as opaque from "@serenity-kit/opaque";

const keyStretching = {
  "argon2id-custom": { memory: 19 * 1024, iterations: 2, parallelism: 1 },
} as const;
type Envelope = {
  version: number;
  sessionId: string;
  counter: number;
  ciphertext: string;
  authenticationTag: string;
};

function secureSession(sessionId: string, key: string, pairing = false) {
  const prefix = pairing ? "gganbu/pairing-" : "gganbu/";
  const derive = (direction: string) =>
    Buffer.from(
      hkdfSync(
        "sha256",
        Buffer.from(key, "base64"),
        Buffer.alloc(0),
        prefix + direction,
        32,
      ),
    );
  let counter = 0;
  const metadata = (count: number) => {
    const bytes = Buffer.alloc(8);
    bytes.writeBigUInt64BE(BigInt(count));
    return {
      nonce: createHash("sha256")
        .update(sessionId)
        .update(bytes)
        .digest()
        .subarray(0, 12),
      aad: Buffer.concat([Buffer.from([1]), Buffer.from(sessionId), bytes]),
    };
  };
  return {
    encrypt(payload: object): Envelope {
      counter += 1;
      const { nonce, aad } = metadata(counter);
      const cipher = createCipheriv(
        "chacha20-poly1305",
        derive("client-to-bridge"),
        nonce,
        { authTagLength: 16 },
      );
      cipher.setAAD(aad);
      const ciphertext = Buffer.concat([
        cipher.update(JSON.stringify(payload)),
        cipher.final(),
      ]);
      return {
        version: 1,
        sessionId,
        counter,
        ciphertext: ciphertext.toString("base64url"),
        authenticationTag: cipher.getAuthTag().toString("base64url"),
      };
    },
    decrypt(envelope: Envelope) {
      const { nonce, aad } = metadata(envelope.counter);
      const cipher = createDecipheriv(
        "chacha20-poly1305",
        derive("bridge-to-client"),
        nonce,
        { authTagLength: 16 },
      );
      cipher.setAAD(aad);
      cipher.setAuthTag(Buffer.from(envelope.authenticationTag, "base64url"));
      return JSON.parse(
        Buffer.concat([
          cipher.update(Buffer.from(envelope.ciphertext, "base64url")),
          cipher.final(),
        ]).toString(),
      ) as Record<string, unknown>;
    },
  };
}

describe.skipIf(!process.env.GGANBU_BRIDGE_TEST_BINARY)(
  "JavaScript client and Rust bridge",
  () => {
    let child: ChildProcess;
    let endpoint: string;
    beforeAll(async () => {
      await opaque.ready;
      child = spawn(process.env.GGANBU_BRIDGE_TEST_BINARY!, [], {
        stdio: ["ignore", "pipe", "inherit"],
      });
      const lines = createInterface({ input: child.stdout! });
      const [line] = await Promise.race([
        once(lines, "line"),
        once(child, "exit").then(() => {
          throw new Error("Rust fixture exited before listening");
        }),
      ]);
      endpoint = String(line);
      lines.close();
    }, 15000);
    afterAll(async () => {
      if (child && child.exitCode === null) {
        const stopped = once(child, "exit");
        child.kill();
        await stopped;
      }
    });
    async function post(path: string, body: object) {
      const response = await fetch(endpoint + path, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(5000),
      });
      expect(response.status, await response.clone().text()).toBe(200);
      return response.json();
    }
    it("pairs, registers, reconnects and exchanges authenticated commands", async () => {
      const password = "12345678";
      const auth = opaque.client.startLogin({ password });
      const authStart = await post("/v1/pairing/auth/start", {
        version: 1,
        message: auth.startLoginRequest,
      });
      const authenticated = opaque.client.finishLogin({
        password,
        keyStretching,
        clientLoginState: auth.clientLoginState,
        loginResponse: authStart.message,
      });
      expect(authenticated).toBeTruthy();
      const authFinish = await post("/v1/pairing/auth/finish", {
        version: 1,
        message: authenticated!.finishLoginRequest,
      });
      const pairing = secureSession(
        authFinish.sessionId,
        authenticated!.sessionKey,
        true,
      );
      const credential = "integration-device-password-with-entropy";
      const registration = opaque.client.startRegistration({
        password: credential,
      });
      const start = pairing.decrypt(
        await post(
          "/v1/pairing/register/start",
          pairing.encrypt({
            deviceId: "integration-device",
            displayName: "Integration browser",
            message: registration.registrationRequest,
          }),
        ),
      );
      const registered = opaque.client.finishRegistration({
        password: credential,
        keyStretching,
        clientRegistrationState: registration.clientRegistrationState,
        registrationResponse: String(start.message),
      });
      const finish = pairing.decrypt(
        await post(
          "/v1/pairing/register/finish",
          pairing.encrypt({ message: registered.registrationRecord }),
        ),
      );
      expect(finish.deviceId).toBe("integration-device");
      const login = opaque.client.startLogin({ password: credential });
      const sessionStart = await post("/v1/session/start", {
        version: 1,
        deviceId: "integration-device",
        message: login.startLoginRequest,
      });
      const loggedIn = opaque.client.finishLogin({
        password: credential,
        keyStretching,
        clientLoginState: login.clientLoginState,
        loginResponse: sessionStart.message,
      });
      expect(loggedIn).toBeTruthy();
      const sessionFinish = await post("/v1/session/finish", {
        version: 1,
        deviceId: "integration-device",
        handshakeId: sessionStart.handshakeId,
        message: loggedIn!.finishLoginRequest,
      });
      const session = secureSession(
        sessionFinish.sessionId,
        loggedIn!.sessionKey,
      );
      const resolved = session.decrypt(
        await post(
          "/v1/actions/resolve",
          session.encrypt({ actions: ["ID_TOGGLE_ENGINE"] }),
        ),
      );
      expect(resolved.bindings).toHaveLength(1);
      const command = session.encrypt({
        action: "ID_TOGGLE_ENGINE",
        requestId: "integration-command",
      });
      expect(
        session.decrypt(await post("/v1/actions/execute", command)).accepted,
      ).toBe(true);
      const replay = await fetch(endpoint + "/v1/actions/execute", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(command),
      });
      expect(replay.status).toBe(401);
      expect(
        session.decrypt(await post("/v1/session/close", session.encrypt({}))),
      ).toEqual({ closed: true });
    }, 15000);
  },
);
