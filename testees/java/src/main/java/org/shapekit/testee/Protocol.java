package org.shapekit.testee;

import java.util.LinkedHashMap;
import java.util.Map;

/** the protocol's message shapes and turning failures into a response status. */
final class Protocol {
  private Protocol() {}

  static final int VERSION = 2;

  /** the sdk version under test has no api for this op or this input. */
  static final class Unsupported extends RuntimeException {
    private static final long serialVersionUID = 1L;

    Unsupported(String reason) {
      super(reason);
    }
  }

  /** the request does not follow the protocol. */
  static final class BadRequest extends RuntimeException {
    private static final long serialVersionUID = 1L;

    BadRequest(String message) {
      super(message);
    }
  }

  /** a throwable from an sdk call, kept apart from the testee's own failures. */
  static final class SdkFailure extends RuntimeException {
    private static final long serialVersionUID = 1L;
    final boolean panic;

    SdkFailure(String call, Throwable cause, boolean panic) {
      super(call + ": " + describe(cause), cause);
      this.panic = panic;
    }

    /** the sdk refusing an input with no sdk call to throw: no decoder, unread bytes. */
    SdkFailure(String message) {
      super(message);
      this.panic = false;
    }
  }

  static String describe(Throwable t) {
    return t.getClass().getName() + ": " + t.getMessage();
  }

  static String handle(Ops ops, String line) {
    String id = "";
    Map<String, Object> resp;
    try {
      Object parsed;
      try {
        parsed = Json.parse(line);
      } catch (Json.ParseError e) {
        throw new BadRequest("request is not json: " + e.getMessage());
      }
      if (!(parsed instanceof Map<?, ?>)) {
        throw new BadRequest("request is not a json object");
      }
      @SuppressWarnings("unchecked")
      Map<String, Object> req = (Map<String, Object>) parsed;
      if (!(req.get("id") instanceof String s)) {
        throw new BadRequest("request has no string id");
      }
      id = s;
      if (!(req.get("op") instanceof String op)) {
        throw new BadRequest("request has no string op");
      }
      Map<String, Object> result = ops.run(new Request(op, req));
      resp = new LinkedHashMap<>();
      resp.put("id", id);
      resp.put("status", "ok");
      resp.put("result", result);
    } catch (Unsupported u) {
      resp = new LinkedHashMap<>();
      resp.put("id", id);
      resp.put("status", "unsupported");
      resp.put("reason", u.getMessage());
    } catch (BadRequest b) {
      resp = error(id, "bad-request", b.getMessage());
    } catch (SdkFailure f) {
      resp = error(id, f.panic ? "panic" : "rejected", f.getMessage());
    } catch (Throwable t) {
      // anything outside an sdk call is the testee's own failure, never an sdk finding
      t.printStackTrace(System.err);
      resp = error(id, "internal", describe(t));
    }
    return Json.write(resp);
  }

  private static Map<String, Object> error(String id, String cls, String message) {
    Map<String, Object> e = new LinkedHashMap<>();
    e.put("class", cls);
    e.put("message", message == null ? "" : message);
    Map<String, Object> resp = new LinkedHashMap<>();
    resp.put("id", id);
    resp.put("status", "error");
    resp.put("error", e);
    return resp;
  }
}
