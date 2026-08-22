// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

// @vitest-environment jsdom
import { beforeAll, describe, expect, it } from "vitest";
import * as opaque from "@serenity-kit/opaque";

describe("OPAQUE browser feasibility", () => {
  beforeAll(async () => {
    await opaque.ready;
  });

  it("completes registration and login in a browser-like runtime", () => {
    const password = "phase-1-test-password";
    const serverSetup = opaque.server.createSetup();
    const registrationStart = opaque.client.startRegistration({ password });
    const { registrationResponse } = opaque.server.createRegistrationResponse({
      serverSetup,
      userIdentifier: "phase-1-device",
      registrationRequest: registrationStart.registrationRequest,
    });
    const registration = opaque.client.finishRegistration({
      clientRegistrationState: registrationStart.clientRegistrationState,
      registrationResponse,
      password,
    });
    const loginStart = opaque.client.startLogin({ password });
    const { loginResponse, serverLoginState } = opaque.server.startLogin({
      serverSetup,
      userIdentifier: "phase-1-device",
      registrationRecord: registration.registrationRecord,
      startLoginRequest: loginStart.startLoginRequest,
    });
    const login = opaque.client.finishLogin({
      clientLoginState: loginStart.clientLoginState,
      loginResponse,
      password,
    });
    expect(login).not.toBeFalsy();
    expect(login?.sessionKey).toEqual(
      opaque.server.finishLogin({
        finishLoginRequest: login!.finishLoginRequest,
        serverLoginState,
      }).sessionKey,
    );
  });

  it("rejects an incorrect password", () => {
    const serverSetup = opaque.server.createSetup();
    const start = opaque.client.startRegistration({ password: "correct" });
    const { registrationResponse } = opaque.server.createRegistrationResponse({
      serverSetup,
      userIdentifier: "phase-1-device",
      registrationRequest: start.registrationRequest,
    });
    const { registrationRecord } = opaque.client.finishRegistration({
      clientRegistrationState: start.clientRegistrationState,
      registrationResponse,
      password: "correct",
    });
    const loginStart = opaque.client.startLogin({ password: "wrong" });
    const { loginResponse } = opaque.server.startLogin({
      serverSetup,
      userIdentifier: "phase-1-device",
      registrationRecord,
      startLoginRequest: loginStart.startLoginRequest,
    });
    expect(
      opaque.client.finishLogin({
        clientLoginState: loginStart.clientLoginState,
        loginResponse,
        password: "wrong",
      }),
    ).toBeFalsy();
  });
});
