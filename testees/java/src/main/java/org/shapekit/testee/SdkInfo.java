package org.shapekit.testee;

import java.net.URISyntaxException;
import java.nio.file.Path;
import java.security.CodeSource;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * the sdk and component versions, read from the jars the classes were loaded from. the sdk jar
 * carries no version of its own (its manifest holds only Manifest-Version, and it has no
 * META-INF/maven entry), so the version is the one in the file name maven's copy-dependencies
 * gives every jar under lib/: artifactId-version.jar, the artifact lib.sha256 pins.
 */
final class SdkInfo {
  static final String PACKAGE = "network.lightsail:stellar-sdk";

  final String version;
  final Map<String, String> components;

  private SdkInfo(String version, Map<String, String> components) {
    this.version = version;
    this.components = components;
  }

  static SdkInfo load() {
    Map<String, String> components = new LinkedHashMap<>();
    // gson reads and writes the sdk's xdr-json and horizon json; commons-codec decodes strkeys
    components.put("com.google.code.gson:gson", jarVersion(load("com.google.gson.Gson"), "gson"));
    components.put(
        "commons-codec:commons-codec",
        jarVersion(load("org.apache.commons.codec.binary.Base32"), "commons-codec"));
    return new SdkInfo(jarVersion(org.stellar.sdk.StrKey.class, "stellar-sdk"), components);
  }

  // the sdk's runtime dependencies are not on the testee's compile path, only on its class path
  private static Class<?> load(String name) {
    try {
      return Class.forName(name);
    } catch (ClassNotFoundException e) {
      throw new IllegalStateException(name + " is not on the class path", e);
    }
  }

  static String jarVersion(Class<?> c, String artifactId) {
    CodeSource source = c.getProtectionDomain().getCodeSource();
    if (source == null || source.getLocation() == null) {
      throw new IllegalStateException(c.getName() + " was not loaded from a jar");
    }
    String file;
    try {
      file = Path.of(source.getLocation().toURI()).getFileName().toString();
    } catch (URISyntaxException e) {
      throw new IllegalStateException(e);
    }
    String prefix = artifactId + "-";
    if (!file.startsWith(prefix) || !file.endsWith(".jar")) {
      throw new IllegalStateException(
          c.getName() + " was loaded from " + file + ", not an " + artifactId + " jar");
    }
    return file.substring(prefix.length(), file.length() - ".jar".length());
  }
}
