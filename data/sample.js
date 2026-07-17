const canvas = document.getElementById("canvas");
const ctx = canvas.getContext("2d");

const FORMATS = {
  millisecond: new Intl.DateTimeFormat("en-US", {
    hour12: false,
    hour: "numeric",
    minute: "numeric",
    second: "numeric",
    fractionalSecondDigits: 3,
  }),
  second: new Intl.DateTimeFormat("en-US", {
    hour12: false,
    hour: "numeric",
    minute: "numeric",
    second: "numeric",
  }),
  minute: new Intl.DateTimeFormat("en-US", {
    hour12: false,
    hour: "numeric",
    minute: "numeric",
  }),
  hour: new Intl.DateTimeFormat("en-US", {
    hour12: false,
    hour: "numeric",
  }),
  day: new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
  }),
  week: new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
  }),
  month: new Intl.DateTimeFormat("en-US", {
    month: "short",
  }),
  quarter: new Intl.DateTimeFormat("en-US", {
    month: "short",
    year: "numeric",
  }),
  year: new Intl.DateTimeFormat("en-US", {
    year: "numeric",
  }),
};

Chart._adapters._date.override({
  formats: () => FORMATS,
  format: function(time, format) {
    return format.format(new Date(time));
  },
  parse: function(value) {
    return value;
  },
  add: function(t1, t2) {
    return t1 + t2;
  },
  diff: function(t1, t2) {
    return t1 - t2;
  },
  startOf: function(time, unit) {
    const date = new Date(time);
    switch (unit) {
    case "millisecond":
      return time;
    case "day":
      return time
        - date.getMilliseconds()
        - (date.getSeconds() * 1000)
        - (date.getMinutes() * 60000)
        - (date.getHours() * 3600000);
    case "week":
    case "isoWeek":
      return time
        - date.getMilliseconds()
        - (date.getSeconds() * 1000)
        - (date.getMinutes() * 60000)
        - (date.getHours() * 3600000)
        - (date.getDay() * 86400000);
    case "month":
      return time
        - date.getMilliseconds()
        - (date.getSeconds() * 1000)
        - (date.getMinutes() * 60000)
        - (date.getHours() * 3600000)
        - (date.getDate() * 86400000);
    }
  },
  // endOf: function(time, unit) {

  // },
});

const chart = new Chart(ctx, {
  type: "line",
  data: {
    datasets: [
      {
        label: "cool",
        data: [[Date.now() - 3600, 4], [Date.now(), 5]]
      },
      {
        label: "bad",
        data: [[Date.now() - 3600, 9], [Date.now(), 10]]
      },
    ],
  },
  options: {
    scales: {
      x: {
        type: "time",
        time: {
          parser: false,
          // unit: "day",
        },
      },
    },
  },
});
