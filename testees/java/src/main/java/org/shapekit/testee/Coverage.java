package org.shapekit.testee;

import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.TreeSet;
import org.jacoco.agent.rt.IAgent;
import org.jacoco.agent.rt.RT;
import org.jacoco.core.JaCoCo;
import org.jacoco.core.analysis.Analyzer;
import org.jacoco.core.analysis.CoverageBuilder;
import org.jacoco.core.analysis.IClassCoverage;
import org.jacoco.core.data.ExecutionData;
import org.jacoco.core.data.ExecutionDataReader;
import org.jacoco.core.data.ExecutionDataStore;
import org.jacoco.core.data.SessionInfoStore;

/**
 * coverage_snapshot and coverage_new from the jacoco agent the runner starts with
 * -javaagent:...=output=none,includes=org.stellar.sdk.*. the agent's probes never reset here,
 * so every snapshot covers the whole process. a range is a source line of an sdk class that
 * ran at least one instruction, `<package path>/<source file>:<line>`
 * (`org/stellar/sdk/xdr/SCVal.java:357`); a coverage_new unit is one jacoco probe.
 */
final class Coverage {
  private final IAgent agent;
  private long lastTotal;

  private Coverage(IAgent agent) {
    this.agent = agent;
  }

  /** the agent when SHAPEKIT_COVERAGE=1, null otherwise; the variable without the agent fails. */
  static Coverage start() {
    if (!"1".equals(System.getenv("SHAPEKIT_COVERAGE"))) {
      return null;
    }
    try {
      return new Coverage(RT.getAgent());
    } catch (NoClassDefFoundError | IllegalStateException e) {
      throw new IllegalStateException(
          "SHAPEKIT_COVERAGE=1 needs the jacoco agent: start java with"
              + " -javaagent:<testee dir>/jacocoagent.jar=output=none,includes=org.stellar.sdk.*",
          e);
    }
  }

  static String version() {
    return JaCoCo.VERSION;
  }

  private ExecutionDataStore read() {
    ExecutionDataStore store = new ExecutionDataStore();
    ExecutionDataReader reader =
        new ExecutionDataReader(new ByteArrayInputStream(agent.getExecutionData(false)));
    reader.setExecutionDataVisitor(store);
    reader.setSessionInfoVisitor(new SessionInfoStore());
    try {
      reader.read();
    } catch (IOException e) {
      throw new IllegalStateException("reading the agent's execution data", e);
    }
    return store;
  }

  Map<String, Object> snapshot() {
    ExecutionDataStore store = read();
    CoverageBuilder builder = new CoverageBuilder();
    Analyzer analyzer = new Analyzer(store, builder);
    for (ExecutionData data : store.getContents()) {
      String resource = data.getName() + ".class";
      try (InputStream in = ClassLoader.getSystemResourceAsStream(resource)) {
        if (in == null) {
          throw new IllegalStateException("no class file for " + data.getName());
        }
        analyzer.analyzeClass(in, data.getName());
      } catch (IOException e) {
        throw new IllegalStateException("analyzing " + data.getName(), e);
      }
    }
    TreeSet<String> ranges = new TreeSet<>();
    for (IClassCoverage c : builder.getClasses()) {
      if (c.getSourceFileName() == null) {
        continue;
      }
      String file = c.getPackageName() + "/" + c.getSourceFileName();
      for (int line = c.getFirstLine(); line > 0 && line <= c.getLastLine(); line++) {
        if (c.getLine(line).getInstructionCounter().getCoveredCount() > 0) {
          ranges.add(file + ":" + line);
        }
      }
    }
    Map<String, Object> r = new LinkedHashMap<>();
    r.put("tool", "jacoco " + JaCoCo.VERSION + " line coverage");
    r.put("ranges", new ArrayList<Object>(ranges));
    return r;
  }

  Map<String, Object> fresh() {
    long total = 0;
    for (ExecutionData data : read().getContents()) {
      for (boolean probe : data.getProbes()) {
        if (probe) {
          total++;
        }
      }
    }
    // probes only ever turn true, so the growth is the number that ran for the first time
    long fresh = total - lastTotal;
    lastTotal = total;
    Map<String, Object> r = new LinkedHashMap<>();
    r.put("new", fresh);
    r.put("total", total);
    return r;
  }
}
