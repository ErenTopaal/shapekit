package org.shapekit.testee;

import java.io.IOException;
import java.lang.reflect.InvocationTargetException;
import org.shapekit.testee.Protocol.SdkFailure;
import org.shapekit.testee.Protocol.Unsupported;

/** runs one call into the sdk and sorts what it throws into the protocol's error classes. */
final class Sdk {
  private Sdk() {}

  @FunctionalInterface
  interface Call<T> {
    T run() throws Exception;
  }

  static <T> T call(String name, Call<T> f) {
    try {
      return f.run();
    } catch (SdkFailure | Unsupported e) {
      throw e;
    } catch (Throwable t) {
      Throwable cause = t;
      // the type table calls the generated decode and fromJson through reflection
      while (cause instanceof InvocationTargetException && cause.getCause() != null) {
        cause = cause.getCause();
      }
      throw new SdkFailure(name, cause, !rejection(cause));
    }
  }

  /**
   * whether a throwable is how the sdk refuses an input. the generated xdr code throws
   * IOException (bad discriminants, lengths, padding, depth, booleans, end of input), the
   * builders and codecs IllegalArgumentException (and its NumberFormatException), the sdk its own
   * SdkException family, gson JsonParseException when json does not fit a type, and the java
   * math it uses ArithmeticException on out-of-range values. anything else, a
   * StackOverflowError, a NullPointerException or a ClassCastException from a generated cast, is
   * the sdk crashing rather than refusing.
   */
  static boolean rejection(Throwable t) {
    return t instanceof IOException
        || t instanceof IllegalArgumentException
        || t instanceof ArithmeticException
        || t instanceof org.stellar.sdk.exception.SdkException
        || isGsonRefusal(t);
  }

  // the testee does not link gson itself: its version follows the sdk's, so the class is
  // recognised by name along the throwable's superclasses
  private static boolean isGsonRefusal(Throwable t) {
    for (Class<?> c = t.getClass(); c != null; c = c.getSuperclass()) {
      if (c.getName().equals("com.google.gson.JsonParseException")) {
        return true;
      }
    }
    return false;
  }
}
