package core

import (
	"bytes"
	"context"
	"io"
	"testing"
	"time"
)

func TestRateLimitedReader_Unlimited(t *testing.T) {
	SetGlobalRateLimitKBps(0)
	data := []byte("hello world this is a test string")
	src := bytes.NewReader(data)
	limited := NewRateLimitedReader(context.Background(), src)

	buf, err := io.ReadAll(limited)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !bytes.Equal(buf, data) {
		t.Errorf("expected %s, got %s", data, buf)
	}
}

func TestRateLimitedReader_Throttle(t *testing.T) {
	// 10 KB of data throttled at 20 KB/s should take at least ~250ms
	SetGlobalRateLimitKBps(20)
	defer SetGlobalRateLimitKBps(0)

	size := 10 * 1024
	data := make([]byte, size)
	for i := range data {
		data[i] = byte(i % 256)
	}

	src := bytes.NewReader(data)
	start := time.Now()
	limited := NewRateLimitedReader(context.Background(), src)

	buf, err := io.ReadAll(limited)
	elapsed := time.Since(start)

	if err != nil {
		t.Fatalf("unexpected error reading: %v", err)
	}
	if len(buf) != size {
		t.Errorf("expected size %d, got %d", size, len(buf))
	}

	if elapsed < 200*time.Millisecond {
		t.Errorf("expected throttled elapsed time >= 200ms, got %v", elapsed)
	}
}

func TestRateLimitedReader_ContextCancel(t *testing.T) {
	SetGlobalRateLimitKBps(1) // 1 KB/s -> very slow
	defer SetGlobalRateLimitKBps(0)

	data := make([]byte, 50*1024)
	src := bytes.NewReader(data)

	ctx, cancel := context.WithCancel(context.Background())
	limited := NewRateLimitedReader(ctx, src)

	go func() {
		time.Sleep(50 * time.Millisecond)
		cancel()
	}()

	buf := make([]byte, 1024)
	var err error
	for {
		_, err = limited.Read(buf)
		if err != nil {
			break
		}
	}

	if err != context.Canceled {
		t.Errorf("expected context.Canceled, got %v", err)
	}
}

func TestGlobalRateLimiter_LiveUpdate(t *testing.T) {
	limiter := NewGlobalRateLimiter(500 * 1024)
	if limiter.GetLimit() != 500*1024 {
		t.Fatalf("expected 500KB/s limit, got %d", limiter.GetLimit())
	}
	limiter.SetLimit(1000 * 1024)
	if limiter.GetLimit() != 1000*1024 {
		t.Fatalf("expected 1000KB/s limit, got %d", limiter.GetLimit())
	}
}
