import matplotlib.pyplot as plt
import pandas as pd
import sys


for i in sys.argv[1:]:
    df = pd.read_csv(i)

    print("statistics:")
    print(f"\tx_mean: {df.x.mean()}")
    print(f"\tx_median: {df.x.median()}")
    print(f"\tx_stddev: {df.x.std()}")

    plt.title(i)
    plt.scatter(df.x, df.y)
    plt.ylabel("queue occupancy")
    plt.xlabel("bitrate (bps)")
    plt.show()
