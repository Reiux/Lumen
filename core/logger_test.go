package core

import (
	"errors"
	"fmt"
	"os"
	"strings"
	"sync"
	"testing"
)

func TestLogBuffer_CapacityRollover(t *testing.T) {
	buf := NewLogBuffer(5)

	for i := 1; i <= 8; i++ {
		buf.AddEntry(LogEntry{
			FileName: fmt.Sprintf("file_%d.jpg", i),
			Level:    LogLevelInfo,
		})
	}

	if buf.Count() != 5 {
		t.Fatalf("expected 5 entries, got %d", buf.Count())
	}

	entries := buf.GetEntries(10, "")
	if len(entries) != 5 {
		t.Fatalf("expected 5 retrieved entries, got %d", len(entries))
	}

	// Reverse chronological: newest first (file_8 down to file_4)
	if entries[0].FileName != "file_8.jpg" {
		t.Errorf("newest entry = %s, want file_8.jpg", entries[0].FileName)
	}
	if entries[4].FileName != "file_4.jpg" {
		t.Errorf("oldest remaining entry = %s, want file_4.jpg", entries[4].FileName)
	}
}

func TestLogBuffer_ConcurrentWrites(t *testing.T) {
	buf := NewLogBuffer(100)
	const numGoroutines = 20
	const entriesPerGoroutine = 50

	var wg sync.WaitGroup
	wg.Add(numGoroutines)

	for g := 0; g < numGoroutines; g++ {
		go func(gid int) {
			defer wg.Done()
			for i := 0; i < entriesPerGoroutine; i++ {
				buf.AddEntry(LogEntry{
					FileName: fmt.Sprintf("g%d_item%d.png", gid, i),
					Level:    LogLevelInfo,
				})
			}
		}(g)
	}

	wg.Wait()

	if buf.Count() != 100 {
		t.Fatalf("expected capped count 100, got %d", buf.Count())
	}

	entries := buf.GetEntries(0, "")
	if len(entries) != 100 {
		t.Fatalf("expected 100 entries, got %d", len(entries))
	}
}

func TestLogBuffer_ProblemFiltering(t *testing.T) {
	buf := NewLogBuffer(20)

	buf.AddEntry(LogEntry{FileName: "ok1.jpg", Level: LogLevelInfo})
	buf.AddEntry(LogEntry{FileName: "warn1.jpg", Level: LogLevelWarn, UserMessage: "Slow network"})
	buf.AddEntry(LogEntry{FileName: "ok2.jpg", Level: LogLevelInfo})
	buf.AddEntry(LogEntry{FileName: "err1.jpg", Level: LogLevelError, UserMessage: "Auth failed"})
	buf.AddEntry(LogEntry{FileName: "ok3.jpg", Level: LogLevelDebug})

	problems := buf.GetProblems()
	if len(problems) != 2 {
		t.Fatalf("expected 2 problems, got %d", len(problems))
	}

	// Newest first: err1.jpg then warn1.jpg
	if problems[0].FileName != "err1.jpg" {
		t.Errorf("first problem = %s, want err1.jpg", problems[0].FileName)
	}
	if problems[1].FileName != "warn1.jpg" {
		t.Errorf("second problem = %s, want warn1.jpg", problems[1].FileName)
	}
}

func TestLogBuffer_SecretRedaction(t *testing.T) {
	rawSecret := "Upload failed: oauth_token=secret_1234567890abcdef and Token=master_token_xyz with Bearer ya29.a0AfH6SMDI_test and aas_et/AK60b_test"
	redacted := RedactSecrets(rawSecret)

	if strings.Contains(redacted, "secret_1234567890abcdef") {
		t.Errorf("redacted string leaked oauth_token: %s", redacted)
	}
	if strings.Contains(redacted, "master_token_xyz") {
		t.Errorf("redacted string leaked master Token: %s", redacted)
	}
	if strings.Contains(redacted, "ya29.a0AfH6SMDI_test") {
		t.Errorf("redacted string leaked Google access token: %s", redacted)
	}
	if strings.Contains(redacted, "aas_et/AK60b_test") {
		t.Errorf("redacted string leaked Android auth token: %s", redacted)
	}

	buf := NewLogBuffer(5)
	buf.AddEntry(LogEntry{
		RawError: rawSecret,
		Level:    LogLevelError,
	})

	entries := buf.GetEntries(1, "")
	if strings.Contains(entries[0].RawError, "secret_1234567890abcdef") {
		t.Errorf("LogBuffer stored raw secret without redaction")
	}
}

func TestMapErrorToProblem(t *testing.T) {
	// 401
	p1 := MapErrorToProblem(errors.New("request failed with status 401: Unauthorized"))
	if p1.ReasonCode != ReasonAuthExpired {
		t.Errorf("expected ReasonAuthExpired, got %s", p1.ReasonCode)
	}

	// 429
	p2 := MapErrorToProblem(errors.New("request failed with status 429: Too Many Requests"))
	if p2.ReasonCode != ReasonRateLimited {
		t.Errorf("expected ReasonRateLimited, got %s", p2.ReasonCode)
	}

	// File not exist
	p3 := MapErrorToProblem(os.ErrNotExist)
	if p3.ReasonCode != ReasonFileUnreadable {
		t.Errorf("expected ReasonFileUnreadable, got %s", p3.ReasonCode)
	}
}
