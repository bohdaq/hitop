package main

import (
	"fmt"
	"io"
	"net/http"
	"strings"
)

func main() {
	body := strings.NewReader("{\"query\":\"query User($id: ID!) {\\n  user(id: $id) { name }\\n}\\n\",\"variables\":{\"id\": \"7\"},\"operationName\":\"User\"}")
	req, err := http.NewRequest("POST", "https://api.example.test/graphql", body)
	if err != nil {
		panic(err)
	}
	req.Header.Add("Authorization", "Bearer {{token}}")
	req.Header.Add("Content-Type", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	out, _ := io.ReadAll(resp.Body)
	fmt.Println(resp.Status)
	fmt.Println(string(out))
}
