package org.shapekit.testee;

import java.io.ByteArrayInputStream;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.HashMap;
import java.util.Map;
import org.shapekit.testee.Protocol.SdkFailure;
import org.shapekit.testee.Protocol.Unsupported;
import org.stellar.sdk.xdr.XdrDataInputStream;
import org.stellar.sdk.xdr.XdrElement;

/**
 * the sdk's xdr classes by the protocol's type names, from the generated table of the linked
 * sdk version, and decoding through their own decode(XdrDataInputStream, int).
 */
final class Types {
  private final String version;
  private final Map<String, String> names = new HashMap<>();

  Types(String version) {
    this.version = version;
    Map<String, String> only = TypesGen.ONLY.get(version);
    if (only == null) {
      throw new IllegalStateException(
          "TypesGen has no table for stellar-sdk " + version + "; run testees/java/gentypes.py");
    }
    names.putAll(TypesGen.COMMON);
    names.putAll(only);
  }

  int size() {
    return names.size();
  }

  Class<?> type(String name) {
    String binary = names.get(name);
    if (binary == null) {
      throw new Unsupported(
          "the testee's type table, generated from the xdr schema against "
              + SdkInfo.PACKAGE
              + " "
              + version
              + ", has no "
              + name);
    }
    try {
      return Class.forName("org.stellar.sdk.xdr." + binary);
    } catch (ClassNotFoundException e) {
      throw new IllegalStateException("the type table names a class the sdk does not have", e);
    }
  }

  static Method method(Class<?> c, String name, Class<?>... params) {
    try {
      Method m = c.getMethod(name, params);
      if (!Modifier.isStatic(m.getModifiers())) {
        throw new IllegalStateException(c.getName() + "." + name + " is not static");
      }
      return m;
    } catch (NoSuchMethodException e) {
      throw new IllegalStateException(c.getName() + " has no " + name, e);
    }
  }

  /**
   * decodes one value with the sdk's limits: maxDepth through decode, the length through
   * setMaxInputLen. the sdk's fromXdrByteArray does not check that the whole input was read,
   * so the testee refuses unread bytes itself, as the protocol requires.
   */
  static XdrElement decode(Class<?> c, byte[] data, int depth, int len) {
    Method decode = method(c, "decode", XdrDataInputStream.class, int.class);
    ByteArrayInputStream bytes = new ByteArrayInputStream(data);
    Object value =
        Sdk.call(
            c.getSimpleName() + ".decode",
            () -> {
              XdrDataInputStream in = new XdrDataInputStream(bytes);
              in.setMaxInputLen(len);
              return decode.invoke(null, in, depth);
            });
    if (bytes.available() != 0) {
      throw new SdkFailure(
          "input not fully consumed: decoded "
              + (data.length - bytes.available())
              + " of "
              + data.length
              + " bytes");
    }
    return (XdrElement) value;
  }

  static byte[] encode(XdrElement value) {
    return Sdk.call(value.getClass().getSimpleName() + ".toXdrByteArray", value::toXdrByteArray);
  }
}
