package tech.rws8.hitop.platform

import com.google.mlkit.genai.common.FeatureStatus
import com.google.mlkit.genai.prompt.Generation
import com.google.mlkit.genai.prompt.GenerativeModel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.mapNotNull
import tech.rws8.hitop.core.LanguageAssistant

/**
 * Gemini Nano on the phone through ML Kit GenAI's Prompt API: the only code that talks to the
 * model. Where the phone doesn't support it the AI features are hidden; nothing leaves the phone.
 */
class GeminiNano : LanguageAssistant {
    private val model: GenerativeModel by lazy { Generation.getClient() }

    override suspend fun availability(): LanguageAssistant.Availability = try {
        when (model.checkStatus()) {
            FeatureStatus.AVAILABLE -> LanguageAssistant.Availability.Ready
            FeatureStatus.DOWNLOADABLE, FeatureStatus.DOWNLOADING -> {
                // The phone supports it: the model downloads in the background (no account, no purchase).
                runCatching { model.download() }
                LanguageAssistant.Availability.Unavailable("The on-device model is getting ready. Try again in a few minutes.")
            }
            else -> LanguageAssistant.Availability.Unavailable("This phone doesn't have the on-device model.")
        }
    } catch (e: Throwable) {
        LanguageAssistant.Availability.Unavailable("This phone doesn't have the on-device model.")
    }

    override fun stream(prompt: String): Flow<String> = model.generateContentStream(prompt).mapNotNull { it.candidates.firstOrNull()?.text }

    override suspend fun answer(prompt: String): String = model.generateContent(prompt).candidates.firstOrNull()?.text.orEmpty()
}
