# /// script
# dependencies = [
#   "httpx",
#   "numpy",
#   "matplotlib",
#   "tabulate",
# ]
# ///
import asyncio
import httpx
import time
import numpy as np
import matplotlib.pyplot as plt
from tabulate import tabulate
import sys

SERVER_URL = "http://localhost:8080"
WAIT_SECONDS = 20
N_VALUES = [1, 10, 100, 500]

async def make_wait_call(client, session_id):
    try:
        resp = await client.get(f"{SERVER_URL}/wait?seconds={WAIT_SECONDS}", timeout=WAIT_SECONDS + 10)
        return resp.status_code == 200
    except Exception as e:
        # print(f"Wait call {session_id} error: {e}")
        return False

async def make_query_call(client):
    start = time.perf_counter()
    try:
        resp = await client.get(f"{SERVER_URL}/query", timeout=10)
        latency = (time.perf_counter() - start) * 1000 # ms
        return latency, resp.status_code == 200
    except Exception as e:
        latency = (time.perf_counter() - start) * 1000
        return latency, False

async def run_test(n):
    print(f"Running test for N={n}...")
    # Increase connection limits to handle N concurrent wait calls
    limits = httpx.Limits(max_connections=n + 10, max_keepalive_connections=10)
    async with httpx.AsyncClient(limits=limits) as client:
        # Start N wait calls
        wait_tasks = [asyncio.create_task(make_wait_call(client, i)) for i in range(n)]

        latencies = []
        errors = 0

        # Give some time for connections to be established
        await asyncio.sleep(1)

        # Sequentially make query calls until one wait call completes
        # Or until some safety timeout (e.g. WAIT_SECONDS + 5)
        start_test = time.time()
        while time.time() - start_test < WAIT_SECONDS + 5:
            # Check if any wait task is done
            done, _ = await asyncio.wait(wait_tasks, timeout=0, return_when=asyncio.FIRST_COMPLETED)
            if done:
                print(f"First wait call completed for N={n}")
                break

            latency, success = await make_query_call(client)
            latencies.append(latency)
            if not success:
                errors += 1

        # Wait for all remaining wait tasks to complete naturally
        print(f"Waiting for remaining {n-1} wait calls to complete...")
        if wait_tasks:
            await asyncio.gather(*wait_tasks, return_exceptions=True)

        if not latencies:
            return {
                "n": n,
                "avg": 0, "std": 0, "min": 0, "max": 0, "error_rate": 1.0, "latencies": [0]
            }

        avg = np.mean(latencies)
        std = np.std(latencies)
        min_val = np.min(latencies)
        max_val = np.max(latencies)
        error_rate = errors / len(latencies)

        return {
            "n": n,
            "avg": avg,
            "std": std,
            "min": min_val,
            "max": max_val,
            "error_rate": error_rate,
            "latencies": latencies
        }

async def main():
    print(f"Starting concurrency test against {SERVER_URL}")
    results = []
    for n in N_VALUES:
        try:
            res = await run_test(n)
            results.append(res)
        except Exception as e:
            print(f"Test failed for N={n}: {e}")
        # Give server a moment to recover
        await asyncio.sleep(5)

    if not results:
        print("No results collected.")
        return

    # Print Table
    table_data = []
    for res in results:
        table_data.append([
            res["n"],
            f"{res['avg']:.2f}",
            f"{res['std']:.2f}",
            f"{res['min']:.2f}",
            f"{res['max']:.2f}",
            f"{res['error_rate']*100:.2f}%",
            len(res["latencies"])
        ])

    headers = ["N", "Avg (ms)", "StdDev (ms)", "Min (ms)", "Max (ms)", "Error Rate", "Queries"]
    print("\nConcurrency Test Results:")
    print(tabulate(table_data, headers=headers, tablefmt="grid"))

    # Generate Plot
    plt.figure(figsize=(12, 6))
    data_to_plot = [res["latencies"] for res in results]
    labels = [f"N={n}" for n in N_VALUES]

    plt.boxplot(data_to_plot, tick_labels=labels, showfliers=False)
    plt.title(f"Query Latency Distribution vs Number of Concurrent Waits (Wait={WAIT_SECONDS}s)")
    plt.xlabel("Number of concurrent /wait requests (N)")
    plt.ylabel("Latency (ms)")
    plt.grid(True, axis='y', linestyle='--', alpha=0.7)

    output_file = "latency_distribution.png"
    plt.savefig(output_file)
    print(f"\nChart saved as {output_file}")

if __name__ == "__main__":
    asyncio.run(main())
