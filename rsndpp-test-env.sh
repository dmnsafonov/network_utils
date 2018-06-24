#! /bin/bash

if [[ "$1" == "setup" ]]; then
    ip link add rsp-inner type veth peer name rsp-mid-in
    ip link add rsp-outer type veth peer name rsp-mid-out

    ip netns add rsp-inner
    ip netns add rsp-middle
    ip netns add rsp-outer

    ip link set rsp-inner netns rsp-inner
    ip link set rsp-mid-in netns rsp-middle
    ip link set rsp-mid-out netns rsp-middle
    ip link set rsp-outer netns rsp-outer

    ip netns exec rsp-inner ip link set lo up
    ip netns exec rsp-middle ip link set lo up
    ip netns exec rsp-outer ip link set lo up

    ip netns exec rsp-inner ip link set rsp-inner up
    ip netns exec rsp-middle ip link set rsp-mid-in up
    ip netns exec rsp-middle ip link set rsp-mid-out up
    ip netns exec rsp-outer ip link set rsp-outer up

    ip netns exec rsp-middle ip addr add fc00::1:ffff/112 dev rsp-mid-in
    ip netns exec rsp-middle ip addr add fc00::2:ffff/64 dev rsp-mid-out

    ip netns exec rsp-inner ip addr add fc00::1:1/112 dev rsp-inner
    ip netns exec rsp-inner ip -6 route add default via fc00::1:ffff dev rsp-inner

    ip netns exec rsp-outer ip addr add fc00::2:1/64 dev rsp-outer

    ip netns exec rsp-inner sysctl -q net.ipv6.conf.all.forwarding=0
    ip netns exec rsp-middle sysctl -q net.ipv6.conf.all.forwarding=1
    ip netns exec rsp-outer sysctl -q net.ipv6.conf.all.forwarding=0
elif [[ "$1" == "clean" ]]; then
    ip netns del rsp-inner
    ip netns del rsp-middle
    ip netns del rsp-outer
elif [[ "$1" == "innershell" ]]; then
    ip netns exec rsp-inner bash --rcfile rsndpp-inner-rc
elif [[ "$1" == "middleshell" ]]; then
    ip netns exec rsp-middle bash --rcfile rsndpp-middle-rc
elif [[ "$1" == "outershell" ]]; then
    ip netns exec rsp-outer bash --rcfile rsndpp-outer-rc
elif [[ "$1" == "kill" ]]; then
    killall -9 rsndpproxy
elif [[ "$1" == "___" ]]; then
    export PS1="($2) $PS1" bash
else
    echo 'use setup, clean, innershell, middleshell, or outershell'
fi
