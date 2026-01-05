//! Web UI templates for the Prompt Enhancer
//! Based on Augment VSCode plugin official templates

/// Web UI HTML template for the Prompt Enhancer
pub const ENHANCER_UI_HTML: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Prompt Enhancer - ACE Tool</title>
  <style>
    * {
      margin: 0;
      padding: 0;
      box-sizing: border-box;
    }

    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', 'Helvetica Neue', sans-serif;
      background: #f5f5f5;
      min-height: 100vh;
      padding: 20px;
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .container {
      background: white;
      border-radius: 8px;
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
      border: 1px solid #e0e0e0;
      max-width: 1000px;
      width: 100%;
      overflow: hidden;
    }

    .header {
      background: white;
      color: #333;
      padding: 30px;
      text-align: center;
      border-bottom: 1px solid #e0e0e0;
    }

    .header h1 {
      font-size: 24px;
      font-weight: 600;
      margin-bottom: 8px;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 10px;
      color: #333;
    }

    .header p {
      font-size: 14px;
      color: #666;
    }

    .countdown {
      margin-top: 12px;
      padding: 8px 16px;
      background: #f0f0f0;
      border-radius: 6px;
      display: inline-block;
      font-size: 13px;
      font-weight: 500;
      color: #555;
    }

    .countdown.warning {
      background: #fff3cd;
      color: #856404;
    }

    .countdown.danger {
      background: #f8d7da;
      color: #721c24;
      animation: pulse 1s ease-in-out infinite;
    }

    @keyframes pulse {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.7; }
    }

    .content {
      padding: 30px;
    }

    .section {
      margin-bottom: 25px;
    }

    .section-title {
      font-size: 14px;
      font-weight: 600;
      color: #333;
      margin-bottom: 10px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }

    .editor-wrapper {
      position: relative;
    }

    textarea {
      width: 100%;
      min-height: 350px;
      padding: 16px;
      border: 2px solid #e0e0e0;
      border-radius: 8px;
      font-family: 'SF Mono', 'Monaco', 'Menlo', 'Consolas', monospace;
      font-size: 14px;
      line-height: 1.6;
      resize: vertical;
      transition: border-color 0.3s;
      background: #fafafa;
    }

    textarea:focus {
      outline: none;
      border-color: #333;
      background: white;
    }

    .char-count {
      position: absolute;
      bottom: 12px;
      right: 12px;
      background: rgba(255, 255, 255, 0.9);
      padding: 4px 10px;
      border-radius: 12px;
      font-size: 12px;
      color: #666;
      pointer-events: none;
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
    }

    .info-box {
      background: #f9f9f9;
      border-left: 4px solid #333;
      padding: 15px;
      border-radius: 4px;
      margin-bottom: 20px;
    }

    .info-box p {
      font-size: 14px;
      color: #555;
      line-height: 1.6;
    }

    .buttons {
      display: flex;
      gap: 12px;
      justify-content: flex-end;
      margin-top: 25px;
    }

    button {
      padding: 12px 28px;
      border: none;
      border-radius: 8px;
      font-size: 15px;
      font-weight: 600;
      cursor: pointer;
      transition: all 0.3s;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .send-btn {
      background: #333;
      color: white;
      box-shadow: none;
    }

    .send-btn:hover:not(:disabled) {
      background: #000;
    }

    .send-btn:active:not(:disabled) {
      background: #000;
    }

    .send-btn:disabled {
      background: #ccc;
      cursor: not-allowed;
      box-shadow: none;
    }

    .cancel-btn {
      background: white;
      color: #666;
      border: 2px solid #e0e0e0;
    }

    .cancel-btn:hover {
      background: #f5f5f5;
      border-color: #ccc;
    }

    .re-enhance-btn {
      background: white;
      color: #333;
      border: 2px solid #333;
    }

    .re-enhance-btn:hover:not(:disabled) {
      background: #f5f5f5;
      border-color: #000;
    }

    .re-enhance-btn:disabled {
      background: #f5f5f5;
      color: #ccc;
      border-color: #e0e0e0;
      cursor: not-allowed;
    }

    .status {
      margin-top: 20px;
      padding: 15px;
      border-radius: 8px;
      display: none;
      animation: slideIn 0.3s ease;
    }

    @keyframes slideIn {
      from {
        opacity: 0;
        transform: translateY(-10px);
      }
      to {
        opacity: 1;
        transform: translateY(0);
      }
    }

    .status.success {
      background: #d4edda;
      color: #155724;
      border-left: 4px solid #28a745;
      display: block;
    }

    .status.error {
      background: #f8d7da;
      color: #721c24;
      border-left: 4px solid #dc3545;
      display: block;
    }

    .loading {
      display: none;
      text-align: center;
      padding: 40px;
    }

    .loading.active {
      display: block;
    }

    .spinner {
      border: 3px solid #f3f3f3;
      border-top: 3px solid #333;
      border-radius: 50%;
      width: 40px;
      height: 40px;
      animation: spin 1s linear infinite;
      margin: 0 auto 15px;
    }

    @keyframes spin {
      0% { transform: rotate(0deg); }
      100% { transform: rotate(360deg); }
    }

    .keyboard-hint {
      font-size: 12px;
      color: #999;
      text-align: center;
      margin-top: 15px;
    }

    .keyboard-hint kbd {
      background: #f5f5f5;
      border: 1px solid #ddd;
      border-radius: 4px;
      padding: 2px 6px;
      font-family: monospace;
      font-size: 11px;
    }

    @media (max-width: 768px) {
      body {
        padding: 10px;
      }

      .header {
        padding: 20px;
      }

      .header h1 {
        font-size: 22px;
      }

      .content {
        padding: 20px;
      }

      textarea {
        min-height: 250px;
        font-size: 13px;
      }

      .buttons {
        flex-direction: column-reverse;
      }

      button {
        width: 100%;
        justify-content: center;
      }
    }
  </style>
</head>
<body>
  <div class="container">
    <div class="header">
      <h1>
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 2L2 7l10 5 10-5-10-5z"/>
          <path d="M2 17l10 5 10-5"/>
          <path d="M2 12l10 5 10-5"/>
        </svg>
        Prompt Enhancer
      </h1>
      <p>Review and refine your enhanced prompt</p>
      <div class="countdown" id="countdown">Loading...</div>
    </div>

    <div class="content">
      <div class="loading" id="loading">
        <div class="spinner"></div>
        <p>Loading your enhanced prompt...</p>
      </div>

      <div id="mainContent" style="display: none;">
        <div class="info-box">
          <p>
            <strong>Tip:</strong> AI has enhanced your prompt based on conversation history and code context.
            You can further edit it in the editor below, then click "Send Enhanced" to continue.
          </p>
        </div>

        <div class="section">
          <div class="section-title">Enhanced Prompt</div>
          <div class="editor-wrapper">
            <textarea
              id="promptText"
              placeholder="Your enhanced prompt will appear here..."
              spellcheck="false"
            ></textarea>
            <div class="char-count" id="charCount">0 chars</div>
          </div>
        </div>

        <div class="buttons">
          <button class="cancel-btn" onclick="endConversation()">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
            End Chat
          </button>
          <button class="re-enhance-btn" id="reEnhanceBtn" onclick="reEnhance()">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="23 4 23 10 17 10"/>
              <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
            </svg>
            Re-enhance
          </button>
          <button class="cancel-btn" onclick="useOriginal()">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M3 12h18M3 6h18M3 18h18"/>
            </svg>
            Use Original
          </button>
          <button class="send-btn" id="sendBtn" onclick="sendPrompt()">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="22" y1="2" x2="11" y2="13"/>
              <polygon points="22 2 15 22 11 13 2 9 22 2"/>
            </svg>
            Send Enhanced
          </button>
        </div>

        <div class="keyboard-hint">
          Shortcuts: <kbd>Ctrl</kbd> + <kbd>Enter</kbd> Send | <kbd>Esc</kbd> End Chat
        </div>

        <div id="status" class="status"></div>
      </div>
    </div>
  </div>

  <script>
    const urlParams = new URLSearchParams(window.location.search);
    const sessionId = urlParams.get('session');
    const promptText = document.getElementById('promptText');
    const charCount = document.getElementById('charCount');
    const loading = document.getElementById('loading');
    const mainContent = document.getElementById('mainContent');
    const countdownEl = document.getElementById('countdown');

    let countdownInterval = null;
    let sessionCreatedAt = null;
    let sessionTimeoutMs = null;

    // Update character count
    function updateCharCount() {
      const count = promptText.value.length;
      charCount.textContent = count + ' chars';
    }

    promptText.addEventListener('input', updateCharCount);

    // Format time display
    function formatTime(ms) {
      const totalSeconds = Math.floor(ms / 1000);
      const minutes = Math.floor(totalSeconds / 60);
      const seconds = totalSeconds % 60;
      return minutes + ':' + seconds.toString().padStart(2, '0');
    }

    // Update countdown display
    function updateCountdown() {
      if (!sessionCreatedAt || !sessionTimeoutMs) return;

      const now = Date.now();
      const elapsed = now - sessionCreatedAt;
      const remaining = sessionTimeoutMs - elapsed;

      if (remaining <= 0) {
        countdownEl.textContent = 'Timed out';
        countdownEl.className = 'countdown danger';
        if (countdownInterval) {
          clearInterval(countdownInterval);
          countdownInterval = null;
        }
        return;
      }

      const remainingMinutes = remaining / 60000;

      // Update styling
      if (remainingMinutes <= 1) {
        countdownEl.className = 'countdown danger';
      } else if (remainingMinutes <= 3) {
        countdownEl.className = 'countdown warning';
      } else {
        countdownEl.className = 'countdown';
      }

      countdownEl.textContent = 'Remaining: ' + formatTime(remaining);
    }

    // Start countdown
    function startCountdown(createdAt, timeoutMs) {
      sessionCreatedAt = createdAt;
      sessionTimeoutMs = timeoutMs;

      updateCountdown();

      if (countdownInterval) {
        clearInterval(countdownInterval);
      }

      countdownInterval = setInterval(updateCountdown, 1000);
    }

    // Keyboard shortcuts
    document.addEventListener('keydown', (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        sendPrompt();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        endConversation();
      }
    });

    // Load session data
    if (!sessionId) {
      loading.style.display = 'none';
      mainContent.style.display = 'block';
      showStatus('Error: No session ID provided', 'error');
    } else {
      loading.classList.add('active');

      fetch('/api/session?session=' + encodeURIComponent(sessionId))
        .then(r => r.json())
        .then(data => {
          if (data.error) {
            throw new Error(data.error);
          }

          promptText.value = data.enhancedPrompt;
          updateCharCount();
          loading.classList.remove('active');
          mainContent.style.display = 'block';
          promptText.focus();

          // Start countdown
          if (data.createdAt && data.timeoutMs) {
            startCountdown(data.createdAt, data.timeoutMs);
          }
        })
        .catch(err => {
          loading.classList.remove('active');
          mainContent.style.display = 'block';
          showStatus('Load failed: ' + err.message, 'error');
        });
    }

    function reEnhance() {
      const currentContent = promptText.value.trim();

      if (!currentContent) {
        showStatus('Please enter content before enhancing', 'error');
        return;
      }

      const reEnhanceBtn = document.getElementById('reEnhanceBtn');
      const sendBtn = document.getElementById('sendBtn');

      reEnhanceBtn.disabled = true;
      sendBtn.disabled = true;
      reEnhanceBtn.innerHTML = '<div class="spinner" style="width: 16px; height: 16px; border-width: 2px; margin: 0;"></div> Enhancing...';

      fetch('/api/re-enhance', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sessionId: sessionId,
          currentPrompt: currentContent
        })
      })
      .then(r => r.json())
      .then(data => {
        if (data.error) {
          throw new Error(data.error);
        }

        promptText.value = data.enhancedPrompt;
        updateCharCount();
        showStatus('Enhancement successful! You can continue editing or send.', 'success');

        reEnhanceBtn.disabled = false;
        sendBtn.disabled = false;
        reEnhanceBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg> Re-enhance';
      })
      .catch(err => {
        showStatus('Enhancement failed: ' + err.message, 'error');
        reEnhanceBtn.disabled = false;
        sendBtn.disabled = false;
        reEnhanceBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg> Re-enhance';
      });
    }

    function sendPrompt() {
      const content = promptText.value.trim();

      if (!content) {
        showStatus('Please enter content before sending', 'error');
        return;
      }

      const sendBtn = document.getElementById('sendBtn');
      const reEnhanceBtn = document.getElementById('reEnhanceBtn');

      sendBtn.disabled = true;
      reEnhanceBtn.disabled = true;
      sendBtn.innerHTML = '<div class="spinner" style="width: 16px; height: 16px; border-width: 2px; margin: 0;"></div> Sending...';

      fetch('/api/submit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sessionId: sessionId, content: content, action: 'send' })
      })
      .then(r => r.json())
      .then(data => {
        if (data.error) {
          throw new Error(data.error);
        }
        showStatus('Sent successfully! Window will close in 2 seconds...', 'success');
        setTimeout(() => window.close(), 2000);
      })
      .catch(err => {
        showStatus('Send failed: ' + err.message, 'error');
        sendBtn.disabled = false;
        reEnhanceBtn.disabled = false;
        sendBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg> Send';
      });
    }

    function useOriginal() {
      if (confirm('Are you sure you want to use the original prompt?')) {
        fetch('/api/submit', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ sessionId: sessionId, content: '', action: 'use_original' })
        })
        .then(r => r.json())
        .then(data => {
          if (data.error) {
            throw new Error(data.error);
          }
          showStatus('Will use original prompt...', 'success');
          setTimeout(() => window.close(), 1000);
        })
        .catch(err => {
          showStatus('Failed: ' + err.message, 'error');
        });
      }
    }

    function endConversation() {
      if (confirm('Are you sure you want to end this conversation?')) {
        fetch('/api/submit', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ sessionId: sessionId, content: '', action: 'end_conversation' })
        })
        .then(r => r.json())
        .then(data => {
          if (data.error) {
            throw new Error(data.error);
          }
          showStatus('Conversation ended', 'success');
          setTimeout(() => window.close(), 1000);
        })
        .catch(err => {
          showStatus('Failed: ' + err.message, 'error');
        });
      }
    }

    function showStatus(message, type) {
      const status = document.getElementById('status');
      status.textContent = message;
      status.className = 'status ' + type;
    }
  </script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // HTML Template Content Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_not_empty() {
        assert!(!ENHANCER_UI_HTML.is_empty());
    }

    #[test]
    fn test_enhancer_ui_html_is_valid_html() {
        assert!(ENHANCER_UI_HTML.starts_with("<!DOCTYPE html>"));
        assert!(ENHANCER_UI_HTML.contains("<html"));
        assert!(ENHANCER_UI_HTML.contains("</html>"));
    }

    #[test]
    fn test_enhancer_ui_html_has_head() {
        assert!(ENHANCER_UI_HTML.contains("<head>"));
        assert!(ENHANCER_UI_HTML.contains("</head>"));
    }

    #[test]
    fn test_enhancer_ui_html_has_body() {
        assert!(ENHANCER_UI_HTML.contains("<body>"));
        assert!(ENHANCER_UI_HTML.contains("</body>"));
    }

    #[test]
    fn test_enhancer_ui_html_has_title() {
        assert!(ENHANCER_UI_HTML.contains("<title>"));
        assert!(ENHANCER_UI_HTML.contains("Prompt Enhancer"));
        assert!(ENHANCER_UI_HTML.contains("ACE Tool"));
    }

    #[test]
    fn test_enhancer_ui_html_has_charset() {
        assert!(ENHANCER_UI_HTML.contains("charset=\"UTF-8\""));
    }

    #[test]
    fn test_enhancer_ui_html_has_viewport() {
        assert!(ENHANCER_UI_HTML.contains("viewport"));
    }

    // ========================================================================
    // Button Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_send_button() {
        assert!(ENHANCER_UI_HTML.contains("sendPrompt"));
        assert!(
            ENHANCER_UI_HTML.contains("Send Enhanced") || ENHANCER_UI_HTML.contains("send-btn")
        );
    }

    #[test]
    fn test_enhancer_ui_html_has_use_original_button() {
        assert!(ENHANCER_UI_HTML.contains("useOriginal"));
        assert!(
            ENHANCER_UI_HTML.contains("Use Original")
                || ENHANCER_UI_HTML.contains("__USE_ORIGINAL__")
        );
    }

    #[test]
    fn test_enhancer_ui_html_has_re_enhance_button() {
        assert!(ENHANCER_UI_HTML.contains("reEnhance"));
        assert!(
            ENHANCER_UI_HTML.contains("Re-enhance") || ENHANCER_UI_HTML.contains("re-enhance-btn")
        );
    }

    #[test]
    fn test_enhancer_ui_html_has_end_conversation_button() {
        assert!(ENHANCER_UI_HTML.contains("endConversation"));
        assert!(
            ENHANCER_UI_HTML.contains("End Chat")
                || ENHANCER_UI_HTML.contains("__END_CONVERSATION__")
        );
    }

    // ========================================================================
    // API Endpoints Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_session_endpoint() {
        assert!(ENHANCER_UI_HTML.contains("/api/session"));
    }

    #[test]
    fn test_enhancer_ui_html_has_submit_endpoint() {
        assert!(ENHANCER_UI_HTML.contains("/api/submit"));
    }

    #[test]
    fn test_enhancer_ui_html_has_re_enhance_endpoint() {
        assert!(ENHANCER_UI_HTML.contains("/api/re-enhance"));
    }

    // ========================================================================
    // JavaScript Function Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_script_tag() {
        assert!(ENHANCER_UI_HTML.contains("<script>"));
        assert!(ENHANCER_UI_HTML.contains("</script>"));
    }

    #[test]
    fn test_enhancer_ui_html_has_countdown_function() {
        assert!(ENHANCER_UI_HTML.contains("updateCountdown"));
    }

    #[test]
    fn test_enhancer_ui_html_has_char_count_function() {
        assert!(ENHANCER_UI_HTML.contains("updateCharCount"));
    }

    #[test]
    fn test_enhancer_ui_html_has_status_function() {
        assert!(ENHANCER_UI_HTML.contains("showStatus"));
    }

    // ========================================================================
    // Keyboard Shortcuts Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_keyboard_shortcuts() {
        assert!(ENHANCER_UI_HTML.contains("keydown"));
        assert!(ENHANCER_UI_HTML.contains("Ctrl") || ENHANCER_UI_HTML.contains("ctrlKey"));
        assert!(ENHANCER_UI_HTML.contains("Enter"));
        assert!(ENHANCER_UI_HTML.contains("Escape") || ENHANCER_UI_HTML.contains("Esc"));
    }

    // ========================================================================
    // CSS Style Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_style_tag() {
        assert!(ENHANCER_UI_HTML.contains("<style>"));
        assert!(ENHANCER_UI_HTML.contains("</style>"));
    }

    #[test]
    fn test_enhancer_ui_html_has_button_styles() {
        assert!(ENHANCER_UI_HTML.contains(".send-btn"));
        assert!(ENHANCER_UI_HTML.contains(".cancel-btn"));
        assert!(ENHANCER_UI_HTML.contains(".re-enhance-btn"));
    }

    #[test]
    fn test_enhancer_ui_html_has_status_styles() {
        assert!(ENHANCER_UI_HTML.contains(".status"));
        assert!(ENHANCER_UI_HTML.contains(".success"));
        assert!(ENHANCER_UI_HTML.contains(".error"));
    }

    #[test]
    fn test_enhancer_ui_html_has_countdown_styles() {
        assert!(ENHANCER_UI_HTML.contains(".countdown"));
        assert!(ENHANCER_UI_HTML.contains(".warning"));
        assert!(ENHANCER_UI_HTML.contains(".danger"));
    }

    #[test]
    fn test_enhancer_ui_html_has_spinner_animation() {
        assert!(ENHANCER_UI_HTML.contains(".spinner"));
        assert!(ENHANCER_UI_HTML.contains("@keyframes"));
    }

    // ========================================================================
    // Textarea Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_textarea() {
        assert!(ENHANCER_UI_HTML.contains("<textarea"));
        assert!(ENHANCER_UI_HTML.contains("promptText"));
    }

    #[test]
    fn test_enhancer_ui_html_has_char_count_display() {
        assert!(ENHANCER_UI_HTML.contains("charCount"));
    }

    // ========================================================================
    // Session ID Handling Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_reads_session_from_url() {
        assert!(ENHANCER_UI_HTML.contains("URLSearchParams"));
        assert!(ENHANCER_UI_HTML.contains("session"));
    }

    #[test]
    fn test_enhancer_ui_html_sends_session_id() {
        assert!(ENHANCER_UI_HTML.contains("sessionId"));
    }

    // ========================================================================
    // Responsive Design Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_media_queries() {
        assert!(ENHANCER_UI_HTML.contains("@media"));
    }

    // ========================================================================
    // Accessibility Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_lang_attribute() {
        assert!(ENHANCER_UI_HTML.contains("lang="));
    }

    // ========================================================================
    // Template Size Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_reasonable_size() {
        // Template should be substantial but not too large
        let size = ENHANCER_UI_HTML.len();
        assert!(size > 5000, "Template seems too small: {} bytes", size);
        assert!(size < 100000, "Template seems too large: {} bytes", size);
    }

    // ========================================================================
    // Action Field Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_use_original_action() {
        assert!(ENHANCER_UI_HTML.contains("action: 'use_original'"));
    }

    #[test]
    fn test_enhancer_ui_html_has_end_conversation_action() {
        assert!(ENHANCER_UI_HTML.contains("action: 'end_conversation'"));
    }

    #[test]
    fn test_enhancer_ui_html_has_send_action() {
        assert!(ENHANCER_UI_HTML.contains("action: 'send'"));
    }

    // ========================================================================
    // Fetch API Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_uses_fetch() {
        assert!(ENHANCER_UI_HTML.contains("fetch("));
    }

    #[test]
    fn test_enhancer_ui_html_uses_json() {
        assert!(ENHANCER_UI_HTML.contains("application/json"));
        assert!(ENHANCER_UI_HTML.contains("JSON.stringify"));
    }

    // ========================================================================
    // Timer/Countdown Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_has_interval() {
        assert!(ENHANCER_UI_HTML.contains("setInterval") || ENHANCER_UI_HTML.contains("interval"));
    }

    #[test]
    fn test_enhancer_ui_html_has_timeout_handling() {
        assert!(ENHANCER_UI_HTML.contains("timeout") || ENHANCER_UI_HTML.contains("Timeout"));
    }

    // ========================================================================
    // Window Close Tests
    // ========================================================================

    #[test]
    fn test_enhancer_ui_html_can_close_window() {
        assert!(ENHANCER_UI_HTML.contains("window.close"));
    }
}
