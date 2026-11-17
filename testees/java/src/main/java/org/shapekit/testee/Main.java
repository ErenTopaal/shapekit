package org.shapekit.testee;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;

/**
 * shapekit java testee: answers testee protocol requests (docs/testee-protocol.md) with
 * java-stellar-sdk, one json object per line on stdin and stdout.
 */
public final class Main {
  private Main() {}

  public static void main(String[] args) throws IOException {
    // stdout carries responses only, so anything the sdk or a library prints goes to stderr
    PrintStream out = System.out;
    System.setOut(System.err);
    Coverage coverage = Coverage.start();
    Ops ops = new Ops(SdkInfo.load(), coverage);
    BufferedReader in =
        new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8), 1 << 20);
    BufferedWriter w = new BufferedWriter(new OutputStreamWriter(out, StandardCharsets.UTF_8));
    String line;
    while ((line = in.readLine()) != null) {
      if (line.isEmpty()) {
        continue;
      }
      w.write(Protocol.handle(ops, line));
      w.write('\n');
      w.flush();
    }
  }
}
