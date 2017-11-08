#! /bin/bash

if [[ "$1" == "setup" ]]; then
    ip link add send-if type veth peer name recv-if

    sysctl -q net.ipv6.conf.send-if.forwarding=0
    ip netns add pingdata-sender
    ip link set send-if netns pingdata-sender
    ip netns exec pingdata-sender ip link set lo up
    ip netns exec pingdata-sender ip link set send-if up
    ip netns exec pingdata-sender ip addr add fc00::1/64 dev send-if

    sysctl -q net.ipv6.conf.recv-if.forwarding=0
    ip netns add pingdata-receiver
    ip link set recv-if netns pingdata-receiver
    ip netns exec pingdata-receiver ip link set lo up
    ip netns exec pingdata-receiver ip link set recv-if up
    ip netns exec pingdata-receiver ip addr add fc00::2/64 dev recv-if
elif [[ "$1" == "clean" ]]; then
    ip netns del pingdata-sender
    ip netns del pingdata-receiver
elif [[ "$1" == "send" ]]; then
    shift
    ip netns exec pingdata-sender "$@"
elif [[ "$1" == "recv" ]]; then
    shift
    ip netns exec pingdata-receiver "$@"
elif [[ "$1" == "permissions" ]]; then
    setcap 'cap_net_raw=p' ping6-datasend/target/debug/ping6-datasend
    setcap 'cap_net_raw=p' ping6-datarecv/target/debug/ping6-datarecv
else
    echo 'use setup, clean, send, recv or permissions'
fi
