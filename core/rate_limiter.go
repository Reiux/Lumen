package core

import (
	"context"
	"io"
	"time"
)

// RateLimitedReader wraps an io.Reader and restricts its throughput to a specified
// bandwidth limit in KB/s. It is context-aware and terminates sleep immediately
// if the context is cancelled.
type RateLimitedReader struct {
	ctx         context.Context
	r           io.Reader
	bytesPerSec int64
	lastRead    time.Time
}

// NewRateLimitedReader creates a new rate-limited reader. If speedLimitKBps <= 0,
// it returns the original reader unchanged.
func NewRateLimitedReader(ctx context.Context, r io.Reader, speedLimitKBps int) io.Reader {
	if speedLimitKBps <= 0 {
		return r
	}
	if ctx == nil {
		ctx = context.Background()
	}
	return &RateLimitedReader{
		ctx:         ctx,
		r:           r,
		bytesPerSec: int64(speedLimitKBps) * 1024,
		lastRead:    time.Now(),
	}
}

// Read reads from the underlying reader and paces execution to stay within
// the configured bandwidth limit.
func (lr *RateLimitedReader) Read(p []byte) (int, error) {
	if lr.bytesPerSec <= 0 {
		return lr.r.Read(p)
	}

	// Limit single chunk size to maintain smooth granular pacing (up to ~64KB or 1/10th sec)
	maxChunk := int(lr.bytesPerSec / 10)
	if maxChunk < 1024 {
		maxChunk = 1024
	}
	if maxChunk > 64*1024 {
		maxChunk = 64 * 1024
	}
	if len(p) > maxChunk {
		p = p[:maxChunk]
	}

	n, err := lr.r.Read(p)
	if n > 0 {
		expectedDuration := time.Duration(n) * time.Second / time.Duration(lr.bytesPerSec)
		elapsed := time.Since(lr.lastRead)
		if expectedDuration > elapsed {
			sleepDuration := expectedDuration - elapsed
			select {
			case <-time.After(sleepDuration):
			case <-lr.ctx.Done():
				return n, lr.ctx.Err()
			}
		}
		lr.lastRead = time.Now()
	}

	return n, err
}
