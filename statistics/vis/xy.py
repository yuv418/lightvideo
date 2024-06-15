import matplotlib.pyplot as plt
import pandas as pd
import sys


for i in sys.argv[1:]:
    df = pd.read_csv(i)

    print("statistics:")
    print(f"\tx_mean: {df.x.mean()}")
    print(f"\tx_median: {df.x.median()}")
    print(f"\tx_stddev: {df.x.std()}")
    
    print(f"\ty_mean: {df.y.mean()}")
    print(f"\ty_median: {df.y.median()}")
    print(f"\ty_stddev: {df.y.std()}")

    plt.title(i)
    plt.scatter(df.x, df.y)
    plt.ylabel("queue occupancy")
    plt.xlabel("bitrate (bps)")
    plt.show()
