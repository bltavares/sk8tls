#!/bin/bash

file=$1
payload="$(
    cd samples 
    jq ".params.textDocument.uri=\"file://$1\"" didChange.json 
)"

# https://github.com/madkins23/lsp-tester
lsp-tester -logLevel=debug -command=./target/debug/sk8tls -mode=client -request=<(echo $payload)
