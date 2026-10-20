package core

import (
	"bytes"
	"context"
	"io"
	"testing"
	"time"
)

func TestRateLimitedReader_Unlimited(t *testing.T) {
	data := []byte("hello world this is a test string")
	src := bytes.NewReader(data)
	limited := NewRateLimitedReader(context.Background(), src, 0)

	buf, err := io.ReadAll(limited)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !bytes.Equal(buf, data) {
		t.Errorf("expected %s, got %s", data, buf)
	}
}

func TestRateLimitedReader_Throttle(t *testing.T) {
	// 10 KB of data throttled at 20 KB/s should take at least ~300ms
	size := 10 * 1024
	data := make([]byte, size)
	for i := range data {
		data[i] = byte(i % 256)
	}

	src := bytes.NewReader(data)
	start := time.Now()
	limited := NewRateLimitedReader(context.Background(), src, 20) // 20 KB/s

	buf, err := io.ReadAll(limited)
	elapsed := time.Since(start)

	if err != nil {
		t.Fatalf("unexpected error reading: %v", err)
	}
	if len(buf) != size {
		t.Errorf("expected size %d, got %d", size, len(buf))
	}

	// At 20 KB/s, 10 KB should take around 500ms (we allow at least 250ms)
	if elapsed < 250*time.Millisecond {
		t.Errorf("expected throttled elapsed time >= 250ms, got %v", elapsed)
	}
}

func TestRateLimitedReader_ContextCancel(t *testing.T) {
	data := make([]byte, 50*1024)
	src := bytes.NewReader(data)

	ctx, cancel := context.WithCancel(context.Background())
	limited := NewRateLimitedReader(ctx, src, 1) // 1 KB/s -> very slow

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
