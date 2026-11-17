package org.shapekit.testee

import java.io.ByteArrayInputStream
import java.io.IOException
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.jacoco.agent.rt.IAgent
import org.jacoco.agent.rt.RT
import org.jacoco.core.JaCoCo
import org.jacoco.core.analysis.Analyzer
import org.jacoco.core.analysis.CoverageBuilder
import org.jacoco.core.data.ExecutionDataReader
import org.jacoco.core.data.ExecutionDataStore
import org.jacoco.core.data.SessionInfoStore

/**
 * coverage_snapshot and coverage_new from the JaCoCo agent the runner starts with
 * -javaagent:...=output=none,includes=com.soneso.stellar.sdk.*, as the Java testee does. The
 * agent's probes never reset here, so every snapshot covers the whole process. A range is a
 * source line of an SDK class that ran at least one instruction,
 * `<package path>/<source file>:<line>` (`com/soneso/stellar/sdk/xdr/SCValXdr.kt:120`); a
 * coverage_new unit is one JaCoCo probe.
 */
class Coverage private constructor(private val agent: IAgent) {
    private var lastTotal = 0L

    companion object {
        /** The agent when SHAPEKIT_COVERAGE=1, null otherwise; the variable without the agent fails. */
        fun start(): Coverage? {
            if (System.getenv("SHAPEKIT_COVERAGE") != "1") return null
            return try {
                Coverage(RT.getAgent())
            } catch (e: NoClassDefFoundError) {
                throw IllegalStateException(NEEDS_AGENT, e)
            } catch (e: IllegalStateException) {
                throw IllegalStateException(NEEDS_AGENT, e)
            }
        }

        private const val NEEDS_AGENT = "SHAPEKIT_COVERAGE=1 needs the jacoco agent: start java with " +
            "-javaagent:<testee dir>/jacocoagent.jar=output=none,includes=com.soneso.stellar.sdk.*"

        val version: String get() = JaCoCo.VERSION
    }

    private fun read(): ExecutionDataStore {
        val store = ExecutionDataStore()
        val reader = ExecutionDataReader(ByteArrayInputStream(agent.getExecutionData(false)))
        reader.setExecutionDataVisitor(store)
        reader.setSessionInfoVisitor(SessionInfoStore())
        try {
            reader.read()
        } catch (e: IOException) {
            throw IllegalStateException("reading the agent's execution data", e)
        }
        return store
    }

    fun snapshot(): JsonObject {
        val store = read()
        val builder = CoverageBuilder()
        val analyzer = Analyzer(store, builder)
        for (data in store.contents) {
            val resource = data.name + ".class"
            val input = ClassLoader.getSystemResourceAsStream(resource)
                ?: throw IllegalStateException("no class file for ${data.name}")
            input.use { analyzer.analyzeClass(it, data.name) }
        }
        val ranges = sortedSetOf<String>()
        for (c in builder.classes) {
            val source = c.sourceFileName ?: continue
            val file = c.packageName + "/" + source
            var line = c.firstLine
            while (line > 0 && line <= c.lastLine) {
                if (c.getLine(line).instructionCounter.coveredCount > 0) ranges.add("$file:$line")
                line++
            }
        }
        return buildJsonObject {
            put("tool", "jacoco ${JaCoCo.VERSION} line coverage")
            put("ranges", JsonArray(ranges.map { JsonPrimitive(it) }))
        }
    }

    fun fresh(): JsonObject {
        var total = 0L
        for (data in read().contents) {
            for (probe in data.probes) if (probe) total++
        }
        // probes only ever turn true, so the growth is the number that ran for the first time
        val fresh = total - lastTotal
        lastTotal = total
        return buildJsonObject {
            put("new", fresh)
            put("total", total)
        }
    }
}
