package main

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"

	"connectrpc.com/connect"

	benchv1 "github.com/anthropics/connect-rust/benches/rpc-go/gen"
	"github.com/anthropics/connect-rust/benches/rpc-go/gen/genconnect"
)

// benchServiceHandler is an echo-style service that reflects payloads back.
type benchServiceHandler struct {
	genconnect.UnimplementedBenchServiceHandler
}

func (h *benchServiceHandler) Unary(
	_ context.Context,
	req *connect.Request[benchv1.BenchRequest],
) (*connect.Response[benchv1.BenchResponse], error) {
	return connect.NewResponse(&benchv1.BenchResponse{
		Payload: req.Msg.Payload,
	}), nil
}

func (h *benchServiceHandler) ServerStream(
	_ context.Context,
	req *connect.Request[benchv1.BenchRequest],
	stream *connect.ServerStream[benchv1.BenchResponse],
) error {
	count := req.Msg.ResponseCount
	for i := int32(0); i < count; i++ {
		if err := stream.Send(&benchv1.BenchResponse{
			Payload: req.Msg.Payload,
		}); err != nil {
			return err
		}
	}
	return nil
}

func (h *benchServiceHandler) ClientStream(
	_ context.Context,
	stream *connect.ClientStream[benchv1.BenchRequest],
) (*connect.Response[benchv1.BenchResponse], error) {
	var lastPayload *benchv1.Payload
	for stream.Receive() {
		lastPayload = stream.Msg().Payload
	}
	if err := stream.Err(); err != nil {
		return nil, err
	}
	return connect.NewResponse(&benchv1.BenchResponse{
		Payload: lastPayload,
	}), nil
}

func (h *benchServiceHandler) LogUnary(
	_ context.Context,
	req *connect.Request[benchv1.LogRequest],
) (*connect.Response[benchv1.LogResponse], error) {
	return connect.NewResponse(&benchv1.LogResponse{
		Count: int32(len(req.Msg.Records)),
	}), nil
}

func (h *benchServiceHandler) LogUnaryOwned(
	_ context.Context,
	req *connect.Request[benchv1.LogRequest],
) (*connect.Response[benchv1.LogResponse], error) {
	return connect.NewResponse(&benchv1.LogResponse{
		Count: int32(len(req.Msg.Records)),
	}), nil
}

func (h *benchServiceHandler) BidiStream(
	_ context.Context,
	stream *connect.BidiStream[benchv1.BenchRequest, benchv1.BenchResponse],
) error {
	for {
		req, err := stream.Receive()
		if errors.Is(err, nil) {
			// continue
		} else {
			// EOF or error
			return nil
		}
		if err := stream.Send(&benchv1.BenchResponse{
			Payload: req.Payload,
		}); err != nil {
			return err
		}
	}
}

func main() {
	mux := http.NewServeMux()
	mux.Handle(genconnect.NewBenchServiceHandler(&benchServiceHandler{}))

	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}

	// Print the address to stdout for the benchmark harness.
	fmt.Println(listener.Addr().String())

	// Enable h2c (HTTP/2 over cleartext) for gRPC support.
	p := new(http.Protocols)
	p.SetHTTP1(true)
	p.SetUnencryptedHTTP2(true)
	srv := &http.Server{Handler: mux, Protocols: p}
	if err := srv.Serve(listener); err != nil && !errors.Is(err, http.ErrServerClosed) {
		panic(err)
	}
}
