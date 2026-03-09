package main

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"sort"
	"strconv"

	"connectrpc.com/connect"
	"github.com/redis/go-redis/v9"

	gen "github.com/anthropics/connect-rust/benches/rpc-go/gen"
	"github.com/anthropics/connect-rust/benches/rpc-go/gen/genconnect"
)

type fortune struct {
	ID      int
	Message string
}

const key = "fortunes"

// queryFortunes does HGETALL on the valkey hash, adds the ephemeral fortune,
// and sorts by message. The go-redis Client is a pooled connection set with
// no per-request serialization — parallel HGETALLs proceed concurrently.
func queryFortunes(ctx context.Context, rdb *redis.Client) ([]fortune, error) {
	raw, err := rdb.HGetAll(ctx, key).Result()
	if err != nil {
		return nil, err
	}
	result := make([]fortune, 0, len(raw)+1)
	for idStr, msg := range raw {
		id, _ := strconv.Atoi(idStr)
		result = append(result, fortune{ID: id, Message: msg})
	}
	result = append(result, fortune{ID: 0, Message: "Additional fortune added at request time."})
	sort.Slice(result, func(i, j int) bool { return result[i].Message < result[j].Message })
	return result, nil
}

type fortuneHandler struct {
	genconnect.UnimplementedFortuneServiceHandler
	rdb *redis.Client
}

func (h *fortuneHandler) GetFortunes(ctx context.Context, _ *connect.Request[gen.GetFortunesRequest]) (*connect.Response[gen.GetFortunesResponse], error) {
	result, err := queryFortunes(ctx, h.rdb)
	if err != nil {
		return nil, connect.NewError(connect.CodeInternal, err)
	}

	fortunes := make([]*gen.Fortune, len(result))
	for i, f := range result {
		fortunes[i] = &gen.Fortune{
			Id:      int32(f.ID),
			Message: f.Message,
		}
	}

	return connect.NewResponse(&gen.GetFortunesResponse{Fortunes: fortunes}), nil
}

func main() {
	if len(os.Args) < 2 {
		panic("usage: fortune <valkey_addr>")
	}
	rdb := redis.NewClient(&redis.Options{Addr: os.Args[1]})

	mux := http.NewServeMux()
	handler := &fortuneHandler{rdb: rdb}
	mux.Handle(genconnect.NewFortuneServiceHandler(handler))

	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}

	fmt.Println(listener.Addr().String())

	p := new(http.Protocols)
	p.SetHTTP1(true)
	p.SetUnencryptedHTTP2(true)
	srv := &http.Server{Handler: mux, Protocols: p}
	if err := srv.Serve(listener); err != nil && !errors.Is(err, http.ErrServerClosed) {
		panic(err)
	}
}
